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
};

mod codec;
pub mod channel;
pub mod timer_reader;
pub mod networker_reader;

use self::channel::Channel;
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
                            let new_channel = Channel::from_socket(
                                socket,
                                &self.handle,
                                &self.neighbors,
                                &self.networker_sender,
                                &self.sm_client,
                            );

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
//    use std::time;
//    use super::*;
//    use ring::signature;
//
//    use security_module::create_security_module;
//
//    use tokio_core::reactor::Core;
//
//    use crypto::identity::*;
//
//    // Create a dummy node mesh, which has `N = 255` nodes.
//    //
//    // The edges from "odd node" point to "even node", in other
//    // word, only "odd node" can initiate a connection.
//    //
//    // For test usage, they use the port from `9000` to `9254`.
//    fn create_dummy_mesh() -> (
//        Vec<SoftwareEd25519Identity>,
//        Vec<Vec<ChannelerNeighborInfo>>
//    ) {
//        const N: u8 = 255;
//
//        let identities = (0..N).iter().map(|byte| {
//            let fixed_byte = FixedByteRandom { byte: 0x00 };
//            let pkcs8 = signature::Ed25519KeyPair::generate_pkcs8(
//                &fixed_byte
//            ).unwrap();
//            SoftwareEd25519Identity::from_pkcs8(&pkcs8).unwrap()
//        }).collect::<Vec<_>>();
//
//        let public_keys = identities.iter().map(|identity| {
//            identity.get_public_key()
//        }).collect::<Vec<_>>();
//
//        assert_eq!(identities.len(), N as usize);
//        assert_eq!(public_keys.len(), N as usize);
//
//        let mut neighbors = Vec::with_capacity(N as usize);
//
//        for i in 0..(N as usize) {
//            let mut neighbor = Vec::new();
//
//            for j in 0..(N as usize) {
//                if i == j { continue; }
//
//                let addr = if i % 2 == 1 && j % 2 == 0 {
//                    Some(format!("127.0.0.1:{}", 9000 + j).parse().unwrap())
//                } else {
//                    None
//                };
//
//                let neighbor_info = ChannelerNeighborInfo {
//                    neighbor_address: ChannelerAddress {
//                        socket_addr: addr,
//                        neighbor_public_key: public_keys[i].clone(),
//                    },
//                    max_channels: 8u32,
//                };
//
//                neighbor.push(neighbor_info);
//            }
//            neighbors.push(neighbor);
//        }
//
//        (identities, neighbors)
//    }
//
//    #[test]
//    fn basic() {
//        let (identities, neighbors_to_add) = create_dummy_mesh();
//
//        assert_eq!(identities.len(), neighbors_to_add.len());
//
//        for idx in 0..identities.len() {
//            ::std::thread::spawn(|| {
//                let mut core = Core::new().unwrap();
//                let handle = core.handle();
//
//                let (sm_handle, mut sm) = create_security_module(identities[i]);
//                let sm_client = sm.new_client();
//
//                let (networker_sender, channeler_receiver) =
//                    mpsc::channel::<ChannelerToNetworker>(0);
//                let (channeler_sender, networker_receiver) =
//                    mpsc::channel::<NetworkerToChanneler>(0);
//
//                let mut tm = TimerModule::new(time::Duration::from_millis(100));
//
//                let addr = format!("127.0.0.1:{}", 9000 + idx).parse().unwrap();
//
//                let (channeler_close_handle, channeler) = Channeler::new(
//                    &addr,
//                    &handle,
//                    tm.create_client(),
//                    networker_sender,
//                    networker_receiver,
//                    sm_client,
//                );
//
//                let mut msg_stream_to_channeler =
//                    neighbors_to_add[idx].map(|neighbor_info| {
//                        NetworkerToChanneler::AddNeighbor { neighbor_info }
//                    }).collect::<Vec<_>>();
//
//                let send_task = channeler_sender.send_all(msg_stream_to_channeler);
//
//                channeler_receiver.for_each(|msg| {
//                    match msg {
//                        ChannelerToNetworker::ChannelOpened(key) => {}
//                        ChannelerToNetworker::ChannelClosed(key) => {}
//                        ChannelerToNetworker::ChannelMessageReceived(msg) => {}
//                    }
//                });
//
//                handle.spawn(sm.map_err(|_| panic!("security module error")));
//                handle.spawn(tm.map_err(|_| panic!("timer module error")));
//                handle.spawn(send_task.map_err(|_| ()));
//                handle.spawn(channeler_receiver.map_err(|_| ()));
//
//                core.run(channeler).expect("channeler error");
//            });
//        }
//    }
//}
//
