//! The Channeler Module.

#![deny(warnings)]

use std::{io, mem, cell::RefCell, rc::Rc};
use std::net::SocketAddr;
use std::collections::HashMap;

use futures::prelude::*;
use futures::sync::{mpsc, oneshot};

use tokio_core::reactor::Handle;
use tokio_core::net::{Incoming, TcpListener};

use crypto::identity::PublicKey;
use utils::CloseHandle;
use security_module::client::SecurityModuleClient;

use timer::messages::FromTimer;
use networker::messages::NetworkerToChanneler;

pub mod types;
pub mod messages;

mod codec;
mod reader;
pub mod channel;

use self::types::*;
use self::messages::*;

use self::channel::{Channel, ChannelError};
use self::reader::{NetworkerReader, TimerReader};

const KEEP_ALIVE_TICKS: usize = 15;

enum ChannelerState {
    Alive,
    Closing(Box<Future<Item = ((), ()), Error = ChannelerError>>),
    Empty,
}

pub struct Channeler {
    handle: Handle,
    state: ChannelerState,
    listener: Incoming, // Listener for incoming TCP connections:

    neighbors: Rc<RefCell<NeighborsTable>>,
    networker_sender: mpsc::Sender<ChannelerToNetworker>,
    // TODO CR: Changing SecurityModuleClient to be a trait.
    sm_client: SecurityModuleClient,

    close_sender: Option<oneshot::Sender<()>>,
    close_receiver: oneshot::Receiver<()>,

    timer_reader_close_handle: Option<CloseHandle>,
    networker_reader_close_handle: Option<CloseHandle>,
}

impl Channeler {
    fn close(&mut self) -> Box<Future<Item = ((), ()), Error = ChannelerError>> {
        let timer_reader_close_handle =
            match mem::replace(&mut self.timer_reader_close_handle, None) {
                None => panic!("call close after close handler consumed, something go wrong"),
                Some(timer_reader_close_handle) => timer_reader_close_handle,
            };

        let networker_reader_close_handle =
            match mem::replace(&mut self.networker_reader_close_handle, None) {
                None => panic!("call close after close handler consumed, something go wrong"),
                Some(networker_reader_close_handle) => networker_reader_close_handle,
            };

        // TODO CR:
        // Maybe we should wait for timer_reader and networker reader to close?
        // the .close() method on both of the handles returns a future that resolves only when
        // closing was successful. We should probably chain those on the returned future from this
        // function.
        let timer_reader_close_fut = timer_reader_close_handle.close().unwrap();
        let networker_reader_close_fut = networker_reader_close_handle.close().unwrap();

        Box::new(
            timer_reader_close_fut
                .join(networker_reader_close_fut)
                .map_err(|e| e.into()),
        )
    }
}

impl Future for Channeler {
    type Item = ();
    type Error = ChannelerError;

    fn poll(&mut self) -> Poll<(), ChannelerError> {
        trace!("poll - {:?}", ::std::time::Instant::now());

        // TODO CR: Maybe we can use loop_fn here?
        // See also: https://docs.rs/futures/*/futures/future/fn.loop_fn.html
        //
        // It could eliminate the unreachable!() part. I'm still trying to figure out ways to eliminate it.
        loop {
            match mem::replace(&mut self.state, ChannelerState::Empty) {
                ChannelerState::Empty => unreachable!(),
                ChannelerState::Alive => {
                    self.state = ChannelerState::Alive;

                    // Handle closing requests:
                    match self.close_receiver.poll()? {
                        Async::NotReady => (),
                        Async::Ready(()) => {
                            debug!("received the closing request, closing");

                            self.state = ChannelerState::Closing(self.close());

                            return Ok(Async::Ready(()));
                        }
                    }

                    // TODO CR: We might be able to use try_ready! here:

                    // Check if we have new connections:
                    match self.listener.poll()? {
                        Async::NotReady => {
                            return Ok(Async::NotReady);
                        }
                        Async::Ready(None) => {
                            warn!("Listener quit, closing");
                            self.state = ChannelerState::Closing(self.close());
                        }
                        Async::Ready(Some((socket, _))) => {
                            let neighbors = self.neighbors.clone();
                            let mut networker_sender = self.networker_sender.clone();

                            let new_channel = Channel::from_socket(
                                socket,
                                &self.handle,
                                Rc::clone(&self.neighbors),
                                &self.networker_sender,
                                &self.sm_client,
                            ).and_then(
                                move |(channel_index, channel_tx, channel)| {
                                    let remote_public_key = channel.remote_public_key();

                                    match neighbors.borrow_mut().get_mut(&remote_public_key) {
                                        None => {
                                            Err(ChannelError::Closed("can't find this neighbor"))
                                        }
                                        Some(neighbor) => {
                                            let msg = ChannelerToNetworker {
                                                remote_public_key,
                                                channel_index,
                                                event: ChannelEvent::Opened,
                                            };

                                            if networker_sender.try_send(msg).is_err() {
                                                Err(ChannelError::SendToNetworkerFailed)
                                            } else {
                                                neighbor.channels.insert(channel_index, channel_tx);
                                                Ok(channel)
                                            }
                                        }
                                    }
                                },
                            );

                            self.handle.spawn(
                                new_channel
                                    .map_err(|e| {
                                        error!("failed to accept a new connection: {:?}", e);
                                    })
                                    .and_then(|channel| {
                                        channel.map_err(|e| warn!("channel closed: {:?}", e))
                                    }),
                            );
                        }
                    }
                }
                ChannelerState::Closing(mut closing_fut) => {
                    // TODO CR: We might be able to use try_ready! here.
                    match closing_fut.poll()? {
                        Async::Ready(_) => {
                            debug!("channeler going down");
                            match mem::replace(&mut self.close_sender, None) {
                                None => {
                                    error!("close sender had been consumed, something went wrong");
                                    return Err(ChannelerError::SendCloseNotificationFailed);
                                }
                                Some(close_sender) => {
                                    if close_sender.send(()).is_err() {
                                        error!("remote close handle deallocated, something may go wrong");
                                    }
                                }
                            }
                            // TODO CR: In this arm of the big match (over ChannelerState) we don't
                            // set self.state = ...
                            // Will this leave self.state = Empty?

                            return Ok(Async::Ready(()));
                        }
                        Async::NotReady => {
                            self.state = ChannelerState::Closing(closing_fut);
                            return Ok(Async::NotReady);
                        }
                    }
                }
            }
        }
    }
}

impl Channeler {
    pub fn new(
        // TODO CR: SocketAddr implements Copy, I think that we don't need to pass it as a reference.
        // See here: https://doc.rust-lang.org/std/net/enum.SocketAddr.html
        addr: &SocketAddr,
        handle: &Handle,
        timer_receiver: mpsc::Receiver<FromTimer>,
        networker_sender: mpsc::Sender<ChannelerToNetworker>,
        networker_receiver: mpsc::Receiver<NetworkerToChanneler>,
        sm_client: SecurityModuleClient,
    ) -> (CloseHandle, Channeler) {
        let neighbors = Rc::new(RefCell::new(HashMap::<PublicKey, ChannelerNeighbor>::new()));

        let (close_handle, (close_sender, close_receiver)) = CloseHandle::new();

        // TODO CR: I think that we are going against Rust's convention by having the new() method
        // return a tuple of objects. Usually new() returns Self.
        // Maybe we should change the name new() to something else? What do you think?
        let (timer_reader_close_handle, timer_reader) = TimerReader::new(
            timer_receiver,
            &handle,
            networker_sender.clone(),
            sm_client.clone(),
            Rc::clone(&neighbors),
        );

        handle.spawn(timer_reader.map_err(|_| ()));

        // TODO CR: See previous CR comment about the new() method for TimerReader. I think it also
        // applies for the NetworkerReader.
        let (networker_reader_close_handle, networker_reader) =
            NetworkerReader::new(networker_receiver, &handle, Rc::clone(&neighbors));

        handle.spawn(networker_reader.map_err(|_| ()));

        let channeler = Channeler {
            handle: handle.clone(),
            state: ChannelerState::Alive,
            // TODO CR: I think that we need to handle the bind() error more gracefully.
            // It is possible that bind() attempt will fail, specifically it sometimes happens if
            // the server crashed and it immediately retries to bind and listen.
            // What do you think we can do to handle this error gracefully?
            listener: TcpListener::bind(&addr, handle).unwrap().incoming(),
            networker_sender,
            neighbors,
            sm_client,
            close_sender: Some(close_sender),
            close_receiver,
            timer_reader_close_handle: Some(timer_reader_close_handle),
            networker_reader_close_handle: Some(networker_reader_close_handle),
        };

        (close_handle, channeler)
    }
}

#[derive(Debug)]
pub enum ChannelerError {
    Io(io::Error),
    CloseReceiverCanceled,
    ClosingTaskCanceled,
    SendCloseNotificationFailed,
    NetworkerClosed,
    // TODO: We should probably start closing too.
    NetworkerPollError,
    TimerClosed,
    // TODO: We should probably start closing too.
    TimerPollError,
}

impl From<io::Error> for ChannelerError {
    fn from(e: io::Error) -> ChannelerError {
        ChannelerError::Io(e)
    }
}

impl From<oneshot::Canceled> for ChannelerError {
    fn from(_e: oneshot::Canceled) -> ChannelerError {
        ChannelerError::CloseReceiverCanceled
    }
}

//
////#[cfg(test)]
////mod tests {
////    use std::thread;
////    // use std::time::{Duration, Instant};
////
////    use ring::signature;
////    use ring::test::rand::FixedByteRandom;
////    use tokio_core::reactor::Core;
////
////    use super::*;
////    use utils.crypto::identity::*;
////    use security::create_security_module;
////
////    fn create_dummy_identity(fixed_byte: u8) -> SoftwareEd25519Identity {
////        let fixed_byte = FixedByteRandom { byte: fixed_byte };
////        let pkcs8 = signature::Ed25519KeyPair::generate_pkcs8(
////            &fixed_byte
////        ).unwrap();
////        SoftwareEd25519Identity::from_pkcs8(&pkcs8).unwrap()
////    }
////
////    fn start_listener() {
////        let server_addr: SocketAddr = "127.0.0.1:9000".parse().unwrap();
////
////        thread::spawn(move || {
////            // Bootstrap
////            let mut core = Core::new().unwrap();
////            let handle = core.handle();
////
////            let (sm_handle, mut sm) = create_security_module(create_dummy_identity(0));
////            let sm_client = sm.new_client();
////
////            // Timer & Channeler
////            let (networker_sender, channeler_receiver) = mpsc::channel::<ChannelerToNetworker>(0);
////            let (mut channeler_sender, networker_receiver) = mpsc::channel::<NetworkerToChanneler>(0);
////
////            let mut timer_module = TimerModule::new(time::Duration::from_millis(100));
////
////            let (_channeler_close_handle, channeler) = Channeler::new(
////                &addr,
////                &handle,
////                timer_module.create_client(),
////                networker_sender,
////                networker_receiver,
////                sm_client,
////            );
////
////            let mock_networker_receiver_part = channeler_receiver.map_err(|_| ()).for_each(|msg| {
////                match msg {
////                    ChannelerToNetworker::ChannelOpened(_) => {
////                    }
////                    ChannelerToNetworker::ChannelClosed(_) => {
////                    }
////                    ChannelerToNetworker::ChannelMessageReceived(msg) => {
////                    }
////                }
////                Ok(())
////            });
////
////            handle.spawn(sm.then(|_| Ok(())));
////            handle.spawn(timer_module.map_err(|_| ()));
////            handle.spawn(mock_networker_receiver_part);
////
////            core.run(channeler).unwrap();
////        });
////    }
////
////    #[test]
////    fn establish_channel() {}
////}
