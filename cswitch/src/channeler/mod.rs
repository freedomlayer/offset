//! The Channeler Module.

#![deny(warnings)]

use std::mem;
use std::net::SocketAddr;
use std::collections::HashMap;

use futures::sync::{mpsc, oneshot};
use futures::{Async, Future, Poll, Stream, IntoFuture};
use futures_mutex::FutMutex;

use tokio_core::reactor::Handle;
use tokio_core::net::TcpListener;

use crypto::uid::Uid;
use crypto::identity::PublicKey;
use inner_messages::{FromTimer, ChannelerToNetworker, NetworkerToChanneler, ChannelerNeighborInfo};
use security_module::security_module_client::SecurityModuleClient;

mod codec;
pub mod channel;
pub mod timer_reader;
pub mod networker_reader;

use self::channel::Channel;
use self::timer_reader::create_timer_reader_future;
use self::networker_reader::create_networker_reader_future;

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

#[allow(unused)]
pub enum ChannelerError {
    CloseReceiverCanceled,
    SendCloseNotificationFailed,
    NetworkerClosed,
    // TODO: We should probably start closing too.
    NetworkerPollError,
    TimerClosed,
    // TODO: We should probably start closing too.
    TimerPollError,
}

enum ChannelerState {
    Alive,
    Closing(Box<Future<Item = (), Error = ChannelerError>>),
    Empty,
}

pub struct Channeler {
    handle: Handle,
    state: ChannelerState,
    listener: TcpListener,
    close_sender: oneshot::Sender<()>,
    close_receiver: oneshot::Receiver<()>,
}

impl Future for Channeler {
    type Item = ();
    type Error = ChannelerError;

    fn poll(&mut self) -> Poll<(), ChannelerError> {
        loop {
            match mem::replace(&mut self.state, ChannelerState::Empty) {
                ChannelerState::Empty => unreachable!(),
                ChannelerState::Alive => {}
                ChannelerState::Closing(closing_fut) => {
                }
            }
        }
    }
}

impl Channeler {
    pub fn new(
        addr: &SocketAddr,
        handle: &Handle,
        timer_receiver: mpsc::Receiver<FromTimer>,
        channeler_sender: mpsc::Sender<ChannelerToNetworker>,
        networker_receiver: mpsc::Receiver<NetworkerToChanneler>,
        sm_client: SecurityModuleClient,
        close_sender: oneshot::Sender<()>,
        close_receiver: oneshot::Receiver<()>
    ) -> Channeler {
        let neighbors = FutMutex::new(HashMap::<PublicKey, ChannelerNeighbor>::new());

        // Start timer reader
        handle.spawn(
            create_timer_reader_future(
                handle.clone(),
                timer_receiver,
                channeler_sender,
                sm_client.clone(),
                neighbors.clone()
            ).map_err(|_| ())
        );

        // Start networker reader
        handle.spawn(
            create_networker_reader_future(
                handle.clone(),
                networker_receiver,
                neighbors.clone()
            ).map_err(|_| ())
        );

        Channeler {
            handle: handle.clone(),
            listener: TcpListener::bind(addr, handle).unwrap(),
            close_sender,
            close_receiver,
        }
    }
}

#[allow(dead_code)]
pub fn create_channeler_future(
    addr: &SocketAddr,
    handle: &Handle,
    timer_receiver: mpsc::Receiver<FromTimer>,
    channeler_sender: mpsc::Sender<ChannelerToNetworker>,
    networker_receiver: mpsc::Receiver<NetworkerToChanneler>,
    sm_client: SecurityModuleClient,
    _close_sender: oneshot::Sender<()>,
    _close_receiver: oneshot::Receiver<()>
) -> impl Future<Item=(), Error=ChannelerError> {

// Create the shared neighbors table
    let neighbors = FutMutex::new(HashMap::<PublicKey, ChannelerNeighbor>::new());

// Start timer reader
    handle.spawn(
        create_timer_reader_future(
            handle.clone(),
            timer_receiver,
            channeler_sender,
            sm_client.clone(),
            neighbors.clone()
        ).map_err(|_| ())
    );

// Start networker reader
    handle.spawn(
        create_networker_reader_future(
            handle.clone(),
            networker_receiver,
            neighbors.clone()
        ).map_err(|_| ())
    );

// Incoming connection handler
    let listener = TcpListener::bind(&addr, handle).unwrap();

    let server_fut = listener.incoming()
        .map_err(|_| ())
        .map(move |(socket, _)| {
            (socket, handle.clone(), neighbors.clone(), channeler_sender.clone(), sm_client.clone())
        })
        .for_each(move |(socket, handle, neighbors, channeler_sender, sm_client)| {
            let handle_inner = handle.clone();
            Channel::from_socket(socket, &handle, &neighbors, &channeler_sender, &sm_client)
                .then(|channel| {
                    match channel {
                        Ok(channel) => {
                            handle_inner.spawn(channel.then(|_| {
                                println!("channel exit");
                                Ok(())
                            }));
                            Ok(())
                        }
                        Err(e) => {
                            println!("{:?}", e);
                            Ok(())
                        }
                    }
                })
        }).into_future();

    handle.spawn(server_fut.then(|_| Ok(())));

//    close_receiver
//        .map_err(|oneshot::Canceled| {
//            warn!("Remote closing handle was canceled!");
//            ChannelerError::CloseReceiverCanceled
//        })
//        .and_then(move |()| {
//            // TODO:
//            // - Send close requests to all tasks here?
//            // - Wait for everyone to close.
//            // - Notify close handle that we finished closing:
//
//            match close_sender.send(()) {
//                Ok(()) => Ok(()),
//                Err(_) => Err(ChannelerError::SendCloseNotificationFailed),
//            }
//        })
    Ok(()).into_future()
}


