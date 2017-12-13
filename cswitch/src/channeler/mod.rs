//! The Channeler Module.

//#![deny(warnings)]
use std::rc::Rc;
use std::cell::RefCell;
use std::collections::HashMap;

use futures::sync::{mpsc, oneshot};
use futures::future::{Future, IntoFuture};

use tokio_core::reactor::Handle;

use async_mutex::AsyncMutex;
use crypto::uid::Uid;
use crypto::identity::PublicKey;
use inner_messages::{FromTimer, ChannelerToNetworker, NetworkerToChanneler, ChannelerNeighborInfo};
use security_module::security_module_client::SecurityModuleClient;
//use close_handle::{CloseHandle, create_close_handle};
//use crypto::rand_values::RandValue;

mod codec;
//mod watcher;
pub mod timer_reader;
//mod networker_reader;
pub mod channel;

use self::timer_reader::create_timer_reader;

const NUM_RAND_VALUES: usize = 16;
const RAND_VALUE_TICKS: usize = 20;
const KEEP_ALIVE_TICKS: usize = 15;

enum ChannelerError {
    CloseReceiverCanceled,
    SendCloseNotificationFailed,
    NetworkerClosed,
    // TODO: We should probably start closing too.
    NetworkerPollError,
    TimerClosed,
    // TODO: We should probably start closing too.
    TimerPollError,
}

pub enum ToChannel {
    TimeTick,
    SendMessage(Vec<u8>),
}

pub struct ChannelerNeighbor {
    pub info: ChannelerNeighborInfo,
    pub channels: Vec<(Uid, mpsc::Sender<ToChannel>)>,
    pub ticks_to_next_conn_attempt: usize,
    pub num_pending_out_conn: usize,
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


fn create_channeler_future(handle: &Handle,
                           timer_receiver: mpsc::Receiver<FromTimer>,
                           networker_sender: mpsc::Sender<ChannelerToNetworker>,
                           networker_receiver: mpsc::Receiver<NetworkerToChanneler>,
                           security_module_client: SecurityModuleClient,
                           close_sender: oneshot::Sender<()>,
                           close_receiver: oneshot::Receiver<()>)
    -> impl Future<Item=(), Error=ChannelerError> {

    // Create the shared neighbors table
    let neighbors = AsyncMutex::new(HashMap::<PublicKey, ChannelerNeighbor>::new());

    // Start timer reader
    handle.spawn(create_timer_reader(handle.clone(),
                                     timer_receiver,
                                     networker_sender,
                                     security_module_client,
                                     neighbors.clone()).map_err(|_| ()));

    // Start networker reader
    // handle.spawn(networker_reader_future(handle,
    //                                      networker_receiver,
    //                                      am_networker_sender,
    //                                      security_module_client,
    //                                      Rc::clone(&rc_crypt_rng),
    //                                      Rc::clone(&rand_values_store),
    //                                      Rc::clone(&neighbors))
    //     .map_err(|_| ()));
//
//    close_receiver
//        .map_err(|oneshot::Canceled| {
//            warn!("Remote closing handle was canceled!");
//            ChannelerError::CloseReceiverCanceled
//        })
//        .and_then(move |()| {
//            // TODO:
//            // - Send close requests to all tasks here?
//            // - Wait for everyone to close.
//
//            // - Notify close handle that we finished closing:
//
//            match close_sender.send(()) {
//                Ok(()) => Ok(()),
//                Err(_) => Err(ChannelerError::SendCloseNotificationFailed),
//            }
//        })
    Ok(()).into_future()
}


