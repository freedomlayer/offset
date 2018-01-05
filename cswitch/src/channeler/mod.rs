//! The Channeler Module.

#![deny(warnings)]

use std::{io, mem};
use std::net::SocketAddr;
use std::collections::HashMap;

use futures::prelude::*;
use futures::sync::{mpsc, oneshot};
use futures_mutex::FutMutex;

use tokio_core::reactor::Handle;
use tokio_core::net::{TcpListener, Incoming};

use crypto::uid::Uid;
use crypto::identity::PublicKey;
use close_handle::{CloseHandle, create_close_handle};
use security_module::security_module_client::SecurityModuleClient;
use inner_messages::{
    FromTimer,
    ChannelerToNetworker,
    NetworkerToChanneler,
    ChannelerNeighborInfo,
    ChannelOpened,
};

mod codec;
pub mod channel;
pub mod timer_reader;
pub mod networker_reader;

use self::channel::{Channel, ChannelError};
use self::timer_reader::TimerReader;
use self::networker_reader::NetworkerReader;

const KEEP_ALIVE_TICKS: usize = 15;

#[derive(Debug)]
pub enum ToChannel {
    TimeTick,
    SendMessage(Vec<u8>),
}

#[derive(Debug)]
pub struct ChannelerNeighbor {
    pub info: ChannelerNeighborInfo,
    pub channels: Vec<(Uid, mpsc::Sender<ToChannel>)>,
    pub remaining_retry_ticks: usize,
    pub num_pending_out_conn: usize,
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
    #[inline]
    fn from(e: io::Error) -> ChannelerError {
        ChannelerError::Io(e)
    }
}

impl From<oneshot::Canceled> for ChannelerError {
    #[inline]
    fn from(_e: oneshot::Canceled) -> ChannelerError {
        ChannelerError::CloseReceiverCanceled
    }
}

enum ChannelerState {
    Alive,
    Closing(Box<Future<Item=((), ()), Error=ChannelerError>>),
    Empty,
}

pub struct Channeler {
    handle: Handle,
    state: ChannelerState,
    listener: Incoming,
    neighbors: FutMutex<HashMap<PublicKey, ChannelerNeighbor>>,
    networker_sender: mpsc::Sender<ChannelerToNetworker>,
    sm_client: SecurityModuleClient,

    close_sender: Option<oneshot::Sender<()>>,
    close_receiver: oneshot::Receiver<()>,

    timer_reader_close_handle: Option<CloseHandle>,
    networker_reader_close_handle: Option<CloseHandle>,
}

impl Channeler {
    #[inline]
    fn close(&mut self) -> Box<Future<Item=((), ()), Error=ChannelerError>> {
        let timer_reader_close_handle =
            match mem::replace(&mut self.timer_reader_close_handle, None) {
                None => {
                    panic!("call close after close handler consumed, something go wrong")
                }
                Some(timer_reader_close_handle) => timer_reader_close_handle,
            };

        let networker_reader_close_handle =
            match mem::replace(&mut self.networker_reader_close_handle, None) {
                None => {
                    panic!("call close after close handler consumed, something go wrong")
                }
                Some(networker_reader_close_handle) => networker_reader_close_handle,
            };

        let timer_reader_close_fut = timer_reader_close_handle.close().unwrap();
        let networker_reader_close_fut = networker_reader_close_handle.close().unwrap();

        Box::new(
            timer_reader_close_fut.join(networker_reader_close_fut)
                .map_err(|e| e.into())
        )
    }
}

impl Future for Channeler {
    type Item = ();
    type Error = ChannelerError;

    fn poll(&mut self) -> Poll<(), ChannelerError> {
        trace!("poll - {:?}", ::std::time::Instant::now());

        loop {
            match mem::replace(&mut self.state, ChannelerState::Empty) {
                ChannelerState::Empty => unreachable!(),
                ChannelerState::Alive => {
                    self.state = ChannelerState::Alive;

                    match self.close_receiver.poll()? {
                        Async::NotReady => (),
                        Async::Ready(()) => {
                            debug!("received the closing request, closing");

                            self.state = ChannelerState::Closing(self.close());

                            return Ok(Async::Ready(()));
                        }
                    }

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
                                &self.neighbors,
                                &self.networker_sender,
                                &self.sm_client,
                            ).and_then(move |(channel_uid, channel_tx, channel)| {
                                let remote_public_key = channel.remote_public_key();

                                neighbors.lock().map_err(|_: ()| {
                                    error!("failed to add new channel");
                                    ChannelError::FutMutex
                                }).and_then(move |mut neighbors| {
                                    match neighbors.get_mut(&remote_public_key) {
                                        None => {
                                            debug!("unknown neighbors");
                                        }
                                        Some(ref_neighbor) => {
                                            if ref_neighbor.channels.is_empty() {
                                                let msg = ChannelerToNetworker::ChannelOpened(
                                                    ChannelOpened {
                                                        remote_public_key: remote_public_key,
                                                        locally_initialized: true,
                                                    }
                                                );
                                                if networker_sender.try_send(msg).is_err() {
                                                    return Err(ChannelError::SendToNetworkerFailed);
                                                }
                                            }
                                            ref_neighbor.channels.push(
                                                (channel_uid, channel_tx)
                                            );
                                        }
                                    }

                                    Ok(channel)
                                })
                            });

                            let handle_for_channel = self.handle.clone();

                            self.handle.spawn(
                                new_channel.map_err(|e| {
                                    error!("failed to accept a new connection: {:?}", e);
                                    ()
                                }).and_then(move |channel| {
                                    handle_for_channel.spawn(channel.map_err(|_| ()));
                                    Ok(())
                                })
                            );
                        }
                    }
                }
                ChannelerState::Closing(mut closing_fut) => {
                    match closing_fut.poll()? {
                        Async::Ready(_) => {
                            debug!("channeler going down");
                            match mem::replace(&mut self.close_sender, None) {
                                None => {
                                    error!("close sender had been consumed, something go wrong");
                                    return Err(ChannelerError::SendCloseNotificationFailed);
                                }
                                Some(close_sender) => {
                                    if close_sender.send(()).is_err() {
                                        error!("remote close handle deallocated, something may go wrong");
                                    }
                                }
                            }

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
        addr:               &SocketAddr,
        handle:             &Handle,
        timer_receiver:     mpsc::Receiver<FromTimer>,
        networker_sender:   mpsc::Sender<ChannelerToNetworker>,
        networker_receiver: mpsc::Receiver<NetworkerToChanneler>,
        sm_client:          SecurityModuleClient,
    ) -> (CloseHandle, Channeler) {
        let neighbors = FutMutex::new(HashMap::<PublicKey, ChannelerNeighbor>::new());

        let (close_handle, (close_sender, close_receiver)) = create_close_handle();

        let (timer_reader_close_handle, timer_reader) = TimerReader::new(
            handle.clone(),
            timer_receiver,
            networker_sender.clone(),
            sm_client.clone(),
            neighbors.clone(),
        );

        handle.spawn(
            timer_reader.map_err(|_| ())
        );

        let (networker_reader_close_handle, networker_reader) = NetworkerReader::new(
            handle.clone(),
            networker_receiver,
            neighbors.clone(),
        );

        handle.spawn(
            networker_reader.map_err(|_| ())
        );

        let channeler = Channeler {
            handle: handle.clone(),
            state: ChannelerState::Alive,
            listener: TcpListener::bind(addr, handle).unwrap().incoming(),
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

//#[cfg(test)]
//mod tests {
//    use std::thread;
//    // use std::time::{Duration, Instant};
//
//    use ring::signature;
//    use ring::test::rand::FixedByteRandom;
//    use tokio_core::reactor::Core;
//
//    use super::*;
//    use crypto::identity::*;
//    use security_module::create_security_module;
//
//    fn create_dummy_identity(fixed_byte: u8) -> SoftwareEd25519Identity {
//        let fixed_byte = FixedByteRandom { byte: fixed_byte };
//        let pkcs8 = signature::Ed25519KeyPair::generate_pkcs8(
//            &fixed_byte
//        ).unwrap();
//        SoftwareEd25519Identity::from_pkcs8(&pkcs8).unwrap()
//    }
//
//    fn start_listener() {
//        let server_addr: SocketAddr = "127.0.0.1:9000".parse().unwrap();
//
//        thread::spawn(move || {
//            // Bootstrap
//            let mut core = Core::new().unwrap();
//            let handle = core.handle();
//
//            let (sm_handle, mut sm) = create_security_module(create_dummy_identity(0));
//            let sm_client = sm.new_client();
//
//            // Timer & Channeler
//            let (networker_sender, channeler_receiver) = mpsc::channel::<ChannelerToNetworker>(0);
//            let (mut channeler_sender, networker_receiver) = mpsc::channel::<NetworkerToChanneler>(0);
//
//            let mut timer_module = TimerModule::new(time::Duration::from_millis(100));
//
//            let (_channeler_close_handle, channeler) = Channeler::new(
//                &addr,
//                &handle,
//                timer_module.create_client(),
//                networker_sender,
//                networker_receiver,
//                sm_client,
//            );
//
//            let mock_networker_receiver_part = channeler_receiver.map_err(|_| ()).for_each(|msg| {
//                match msg {
//                    ChannelerToNetworker::ChannelOpened(_) => {
//                    }
//                    ChannelerToNetworker::ChannelClosed(_) => {
//                    }
//                    ChannelerToNetworker::ChannelMessageReceived(msg) => {
//                    }
//                }
//                Ok(())
//            });
//
//            handle.spawn(sm.then(|_| Ok(())));
//            handle.spawn(timer_module.map_err(|_| ()));
//            handle.spawn(mock_networker_receiver_part);
//
//            core.run(channeler).unwrap();
//        });
//    }
//
//    #[test]
//    fn establish_channel() {}
//}
