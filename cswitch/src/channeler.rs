extern crate futures;
extern crate rand;


use std::collections::{HashMap};
use std::mem;

use self::rand::Rng;

use self::futures::{Stream, Poll, Async, AsyncSink, StartSend};
use self::futures::future::{Future};
use self::futures::sync::mpsc;
use self::futures::sync::oneshot;



use ::identity::PublicKey;
use ::inner_messages::{FromTimer, ChannelerToNetworker,
    NetworkerToChanneler, ToSecurityModule, FromSecurityModule,
    ChannelerNeighborInfo, ServerType};
use ::close_handle::{CloseHandle, create_close_handle};
use ::rand_values::RandValuesStore;

const NUM_RAND_VALUES: usize = 16;
const RAND_VALUE_TICKS: usize = 20;


const KEEP_ALIVE_TICKS: u64 = 15;



enum ChannelerError {
    CloseReceiverCanceled,
    SendCloseNotificationFailed,
    NetworkerClosed,
    NetworkerPollError,
}

/// A future that resolves when a TCP connection is established.
struct PendingConnection {
}

struct ChannelerConnection {
    // Should contain inner state:
    
    ticks_to_keep_alive: u64,
    // - Last random value?
}


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
    Closed,
}

struct Channeler<R> {
    timer_receiver: mpsc::Receiver<FromTimer>, 
    networker_sender: mpsc::Sender<ChannelerToNetworker>,
    networker_receiver: mpsc::Receiver<NetworkerToChanneler>,
    security_module_sender: mpsc::Sender<ToSecurityModule>,
    security_module_receiver: mpsc::Receiver<FromSecurityModule>,


    close_sender_opt: Option<oneshot::Sender<()>>,
    close_receiver: oneshot::Receiver<()>,

    rand_values_store: RandValuesStore<R>,

    pending_connections: Vec<PendingConnection>,
    unverified_connections: Vec<ChannelerConnection>,
    active_connections: HashMap<PublicKey, Vec<ChannelerConnection>>,
    neighbor_relations: HashMap<PublicKey, ChannelerNeighborInfo>,
    server_type: ServerType,

    state: ChannelerState,
}


impl<R:Rng> Channeler<R> {
    fn create(timer_receiver: mpsc::Receiver<FromTimer>, 
            networker_sender: mpsc::Sender<ChannelerToNetworker>,
            networker_receiver: mpsc::Receiver<NetworkerToChanneler>,
            security_module_sender: mpsc::Sender<ToSecurityModule>,
            security_module_receiver: mpsc::Receiver<FromSecurityModule>,
            crypt_rng: R,
            close_sender: oneshot::Sender<()>,
            close_receiver: oneshot::Receiver<()>) -> Self {

        Channeler {
            timer_receiver,
            networker_sender,
            networker_receiver,
            security_module_sender,
            security_module_receiver,
            close_sender_opt: Some(close_sender),
            close_receiver,
            rand_values_store: RandValuesStore::new(
                crypt_rng, RAND_VALUE_TICKS, NUM_RAND_VALUES),
            pending_connections: Vec::new(),
            unverified_connections: Vec::new(),
            active_connections: HashMap::new(),
            neighbor_relations: HashMap::new(),
            server_type: ServerType::PrivateServer,
            state: ChannelerState::ReadTimer,
        }
    }


    fn check_should_close(&mut self) -> Result<bool, ChannelerError> {
        match self.close_receiver.poll() {
            Ok(Async::Ready(())) => {
                // We were asked to close
                Ok(true)
            },
            Ok(Async::NotReady) => Ok(false),
            Err(_e) => return Err(ChannelerError::CloseReceiverCanceled),
        }
    }

    /// Tell the handle that we are done closing.
    fn send_close_notification(&mut self) -> Result<(), ChannelerError> {
        let close_sender = match self.close_sender_opt.take() {
            None => panic!("Close notification already sent!"),
            Some(close_sender) => close_sender,
        };

        match close_sender.send(()) {
            Ok(()) => Ok(()),
            Err(_) => Err(ChannelerError::SendCloseNotificationFailed),
        }
    }

    fn handle_networker_message(&mut self, message: NetworkerToChanneler) ->
        StartSend<NetworkerToChanneler, ChannelerError> {

        match message {
            NetworkerToChanneler::SendChannelMessage { 
                neighbor_public_key, message_content } => {
                // TODO: Attempt to send a message to some TCP connections leading to
                // the requested neighbor. The chosen Sink could be not ready.
                Ok(AsyncSink::Ready)
            },
            NetworkerToChanneler::AddNeighborRelation { neighbor_info } => {
                let neighbor_public_key = neighbor_info.neighbor_public_key.clone();
                if self.neighbor_relations.contains_key(&neighbor_public_key) {
                    warn!("Neighbor with public key {:?} already exists", 
                          neighbor_public_key);
                }
                self.neighbor_relations.insert(neighbor_public_key, neighbor_info);
                Ok(AsyncSink::Ready)
            },
            NetworkerToChanneler::RemoveNeighborRelation { neighbor_public_key } => {
                match self.neighbor_relations.remove(&neighbor_public_key) {
                    None => warn!("Attempt to remove a nonexistent neighbor \
                        relation with public key {:?}", neighbor_public_key),
                    _ => {},
                };
                // TODO: Possibly close all connections here.

                Ok(AsyncSink::Ready)
            },
            NetworkerToChanneler::SetMaxChannels 
                { neighbor_public_key, max_channels } => {
                match self.neighbor_relations.get_mut(&neighbor_public_key) {
                    None => warn!("Attempt to change max_channels for a \
                        nonexistent neighbor relation with public key {:?}",
                        neighbor_public_key),
                    Some(neighbor_relation) => {
                        neighbor_relation.max_channels = max_channels;
                    },
                };
                Ok(AsyncSink::Ready)
            },
            NetworkerToChanneler::SetServerType(server_type) => {
                self.server_type = server_type;
                Ok(AsyncSink::Ready)
            },
        }

    }
}

impl<R:Rng> Future for Channeler<R> {
    type Item = ();
    type Error = ChannelerError;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        let mut visited_read_close = false;
        loop {
            match mem::replace(&mut self.state, ChannelerState::Closed) {
                ChannelerState::Closed => panic!("Invalid state!"),
                ChannelerState::ReadClose => {
                    let visited_read_close = if visited_read_close {
                        return Ok(Async::NotReady);
                    } else {
                        true
                    };

                    if self.check_should_close()? {
                        self.state = ChannelerState::HandleClose;
                        continue;
                    }
                },
                ChannelerState::HandleClose => {
                    // TODO: Close stuff here.
                    self.send_close_notification()?;
                    self.state = ChannelerState::Closed;
                    return Ok(Async::Ready(()));

                },
                ChannelerState::ReadTimer => {
                    // TODO: 
                    // - If enough time has passed, open a new connection in cases where the
                    //  amount of connections is too small or 0.
                    // - Generate a new random value
                    // - Send connection keepalives?
                    // - Close connections that didn't send a keepalive?
                },

                ChannelerState::ReadNetworker => {

                    match self.networker_receiver.poll() {
                        Ok(Async::Ready(Some(msg))) => {
                            self.state = ChannelerState::HandleNetworker(msg);
                            continue;
                        },
                        Ok(Async::Ready(None)) => {
                            return Err(ChannelerError::NetworkerClosed);
                        },
                        Ok(Async::NotReady) => {},
                        // TODO: What kind of error is e?
                        Err(e) => return Err(ChannelerError::NetworkerPollError),
                    };

                    self.state = ChannelerState::ReadSecurityModule;
                    continue;
                },

                ChannelerState::HandleNetworker(message) => {
                    self.state = match self.handle_networker_message(message)? {
                        AsyncSink::Ready => ChannelerState::ReadSecurityModule,
                        AsyncSink::NotReady(message) => 
                            ChannelerState::HandleNetworker(message),
                    };
                    continue;
                }


                ChannelerState::ReadSecurityModule => {
                    // TODO
                },
                ChannelerState::PollPendingConnection => {
                    // TODO
                },
                ChannelerState::ReadConnectionMessage(i) => {
                    // TODO
                },
                ChannelerState::HandleConnectionMessage(i) => {
                    // TODO
                },
            }
        }
    }
}

