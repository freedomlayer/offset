mod prefix_frame_codec;
mod timer_reader;

extern crate futures;
// extern crate rand;
extern crate tokio_core;
extern crate tokio_io;
extern crate ring;


use std::collections::{HashMap};
use std::mem;
use std::cell::RefCell;

// use self::rand::Rng;

use self::futures::{Stream, Poll, Async, AsyncSink, StartSend};
use self::futures::future::{Future, loop_fn, Loop, LoopFn};
use self::futures::sync::mpsc;
use self::futures::sync::oneshot;
use self::tokio_core::net::TcpStream;
use self::tokio_core::reactor::Handle;
use self::tokio_io::AsyncRead;
use self::ring::rand::SecureRandom;


use ::crypto::identity::PublicKey;
use ::inner_messages::{FromTimer, ChannelerToNetworker,
    NetworkerToChanneler, ToSecurityModule, FromSecurityModule,
    ChannelerNeighborInfo, ServerType};
use ::close_handle::{CloseHandle, create_close_handle};
use ::crypto::rand_values::{RandValuesStore, RandValue};
use self::prefix_frame_codec::PrefixFrameCodec;

const NUM_RAND_VALUES: usize = 16;
const RAND_VALUE_TICKS: usize = 20;


const KEEP_ALIVE_TICKS: usize = 15;



enum ChannelerError {
    CloseReceiverCanceled,
    SendCloseNotificationFailed,
    NetworkerClosed, // TODO: We should probably start closing too.
    NetworkerPollError,
    TimerClosed, // TODO: We should probably start closing too.
    TimerPollError,
}


struct Channel {
    ticks_to_receive_keep_alive: usize,
    ticks_to_send_keep_alive: usize,
    // TODO:
    // - Sender
    // - Receiver
}

struct ChannelerNeighbor {
    info: ChannelerNeighborInfo,
    last_remote_rand_value: Option<RandValue>,
    channels: Vec<Channel>,
    ticks_to_next_conn_attempt: usize,
    num_pending_out_conn: usize,
}


/*
enum ChannelerState {
    ReadClose,
    HandleClose,
    ReadTimer,
    ReadNetworker,
    HandleNetworker(NetworkerToChanneler),
    ReadSecurityModule,
    PollPendingConnection,
    ReadConnectionMessage(usize),
    HandleConnectionMessage(usize),
    // ReadListenSocket,
    Closed,
}
*/

struct InnerChanneler<'a,R:'a> {
    handle: Handle,
    timer_receiver: mpsc::Receiver<FromTimer>, 
    networker_sender: mpsc::Sender<ChannelerToNetworker>,
    networker_receiver: mpsc::Receiver<NetworkerToChanneler>,
    security_module_sender: mpsc::Sender<ToSecurityModule>,
    security_module_receiver: mpsc::Receiver<FromSecurityModule>,
    crypt_rng: &'a R,

    rand_values_store: RandValuesStore,

    neighbors: HashMap<PublicKey, ChannelerNeighbor>,
    server_type: ServerType,

    // state: ChannelerState,
}


struct Channeler<'a, R:'a> {
    inner_channeler: RefCell<InnerChanneler<'a,R>>,
}


fn create_channeler_future<'a,R: SecureRandom>(handle: &Handle, 
            timer_receiver: mpsc::Receiver<FromTimer>, 
            networker_sender: mpsc::Sender<ChannelerToNetworker>,
            networker_receiver: mpsc::Receiver<NetworkerToChanneler>,
            security_module_sender: mpsc::Sender<ToSecurityModule>,
            security_module_receiver: mpsc::Receiver<FromSecurityModule>,
            crypt_rng: &'a R,
            close_sender: oneshot::Sender<()>,
            close_receiver: oneshot::Receiver<()>) -> 
                impl Future<Item=(), Error=ChannelerError> + 'a {

    let rand_values_store = RandValuesStore::new(
        crypt_rng, RAND_VALUE_TICKS, NUM_RAND_VALUES);

    let inner_channeler = RefCell::new(InnerChanneler {
        handle: handle.clone(),
        timer_receiver,
        networker_sender,
        networker_receiver,
        security_module_sender,
        security_module_receiver,
        crypt_rng,
        rand_values_store,
        neighbors: HashMap::new(),
        server_type: ServerType::PrivateServer,
    });

    // TODO: Start all the tasks here:
    // - Start timer reader
    // - start networker reader
    // - start security module client.

    close_receiver
        .map_err(|oneshot::Canceled| {
            warn!("Remote closing handle was canceled!");
            ChannelerError::CloseReceiverCanceled
        })
        .and_then(move |()| {
            // TODO: 
            // - Send close requests to all tasks here?
            // - Wait for everyone to close.
            
            // - Notify close hande that we finished closing:

            match close_sender.send(()) {
                Ok(()) => Ok(()),
                Err(_) => Err(ChannelerError::SendCloseNotificationFailed),
            }
        })
}

struct SecurityModuleClient;

