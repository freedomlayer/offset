//! The Channeler Module.

use std::rc::Rc;
use std::cell::RefCell;
use std::borrow::Borrow;
use std::collections::HashMap;

use futures::sync::{mpsc, oneshot};
use futures::future::{Future, loop_fn, Loop, LoopFn};
use futures::{Stream, Poll, Async, AsyncSink, StartSend};

use tokio_core::reactor::Handle;

use ring::rand::SecureRandom;

use crypto::identity::PublicKey;
use inner_messages::{FromTimer, ChannelerToNetworker, NetworkerToChanneler,
                     ToSecurityModule, FromSecurityModule,
                     ChannelerNeighborInfo, ServerType};
use security_module::security_module_client::SecurityModuleClient;
use close_handle::{CloseHandle, create_close_handle};
use crypto::rand_values::{RandValuesStore, RandValue};
use async_mutex::AsyncMutex;

mod prefix_frame_codec;
mod timer_reader;
pub mod channel;

use self::timer_reader::timer_reader_future;


const NUM_RAND_VALUES: usize  = 16;
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

pub enum ToChannel {
    TimeTick,
    SendMessage(Vec<u8>),
}

pub struct ChannelerNeighbor {
    info: ChannelerNeighborInfo,
    last_remote_rand_value: Option<RandValue>,
    channel_senders: Vec<mpsc::Sender<ToChannel>>,
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

/*
struct InnerChanneler<R> {
    handle: Handle,
    am_networker_sender: AsyncMutex<mpsc::Sender<ChannelerToNetworker>>,
    security_module_client: SecurityModuleClient,
    crypt_rng: Rc<R>,

    rand_values_store: RandValuesStore,

    neighbors: HashMap<PublicKey, ChannelerNeighbor>,
    server_type: ServerType,

    // state: ChannelerState,
}

struct Channeler<R> {
    inner_channeler: RefCell<InnerChanneler<R>>,
}
*/


fn create_channeler_future<R: SecureRandom + 'static>(handle: &Handle, 
            timer_receiver: mpsc::Receiver<FromTimer>, 
            networker_sender: mpsc::Sender<ChannelerToNetworker>,
            networker_receiver: mpsc::Receiver<NetworkerToChanneler>,
            security_module_client: SecurityModuleClient,
            rc_crypt_rng: Rc<R>,
            close_sender: oneshot::Sender<()>,
            close_receiver: oneshot::Receiver<()>) -> 
                impl Future<Item=(), Error=ChannelerError> {

    // Prepare structures to be shared between spawned futures:
    let rand_values_store = Rc::new(RefCell::new(
            RandValuesStore::new::<R>(rc_crypt_rng.borrow(), RAND_VALUE_TICKS, NUM_RAND_VALUES)
    ));

    let networker_sender =  networker_sender;
    let neighbors = Rc::new(RefCell::new(HashMap::<PublicKey, ChannelerNeighbor>::new()));
    let server_type = ServerType::PrivateServer;

    // TODO: Start all the tasks here:
    handle.spawn(timer_reader_future(handle.clone(), 
                               timer_receiver,
                               networker_sender, 
                               security_module_client, 
                               Rc::clone(&rc_crypt_rng), 
                               Rc::clone(&rand_values_store), 
                               Rc::clone(&neighbors))
                 .map_err(|_| ()));

    /*
    handle.spawn(networker_reader_future(handle, 
                               networker_receiver,
                               am_networker_sender, 
                               security_module_client, 
                               Rc::clone(&rc_crypt_rng), 
                               Rc::clone(&rand_values_store), 
                               Rc::clone(&neighbors))
                 .map_err(|_| ()));
     */

    close_receiver
        .map_err(|oneshot::Canceled| {
            warn!("Remote closing handle was canceled!");
            ChannelerError::CloseReceiverCanceled
        })
        .and_then(move |()| {
            // TODO: 
            // - Send close requests to all tasks here?
            // - Wait for everyone to close.

            // - Notify close handle that we finished closing:

            match close_sender.send(()) {
                Ok(()) => Ok(()),
                Err(_) => Err(ChannelerError::SendCloseNotificationFailed),
            }
        })
}


