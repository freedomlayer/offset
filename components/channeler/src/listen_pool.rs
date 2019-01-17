use std::hash::Hash;
use std::collections::{HashMap, HashSet};
use futures::{future, stream, Stream, StreamExt, Sink, SinkExt};
use futures::channel::mpsc;
use futures::task::{Spawn, SpawnExt};

use common::conn::{Listener, FutTransform};
use common::access_control::{AccessControlOp, AccessControl};

use crypto::identity::PublicKey;
use crate::types::{RawConn, AccessControlPk, AccessControlOpPk};
use crate::transform_pool::create_transform_pool;

pub enum LpConfig<B> {
    SetLocalAddresses(Vec<B>),
    UpdateFriend((PublicKey, Vec<B>)),
    RemoveFriend(PublicKey),
}

#[derive(Debug)]
struct ListenPoolClientError;


pub struct LpConfigClient<B> {
    request_sender: mpsc::Sender<LpConfig<B>>,
}

impl<B> LpConfigClient<B> {
    pub fn new(request_sender: mpsc::Sender<LpConfig<B>>) -> Self {
        LpConfigClient {
            request_sender,
        }
    }

    pub async fn config(&mut self, config: LpConfig<B>) -> Result<(), ListenPoolClientError> {
        await!(self.request_sender.send(config))
            .map_err(|_| ListenPoolClientError)?;
        Ok(())
    }
}

#[derive(Debug)]
enum ListenPoolError {
    ConfigClosed,
    TimerClosed,
    SpawnError,
    CreateEncryptPoolError,
}

enum LpEvent<B> {
    Config(LpConfig<B>),
    ConfigClosed,
    IncomingEncryptedConn((PublicKey, RawConn)), // Incoming encrypted connection
    RelayClosed(B),
    TimerTick,
    TimerClosed,
}

enum RelayStatus {
    Waiting(usize), // ticks left to start listening again
    Connected(mpsc::Sender<AccessControlOpPk>),
}

struct Relay {
    friends: HashSet<PublicKey>,
    status: RelayStatus,
}

struct ListenPool<B,L,S> {
    local_addresses: HashSet<B>,
    // friends: HashMap<PublicKey, HashSet<B>>,
    relays: HashMap<B, Relay>,
    plain_conn_sender: mpsc::Sender<(PublicKey, RawConn)>,
    listener: L,
    backoff_ticks: usize,
    spawner: S,
}

impl<B,L,S> ListenPool<B,L,S> 
where
    B: Hash + Eq + Clone,
    L: Listener<Connection=(PublicKey, RawConn), 
        Config=AccessControlOpPk, Arg=(B, AccessControlPk)> + Clone + 'static,
    S: Spawn,
{
    pub fn new(plain_conn_sender: mpsc::Sender<(PublicKey, RawConn)>,
           listener: L,
           backoff_ticks: usize,
           spawner: S) -> Self {

        ListenPool {
            local_addresses: HashSet::new(),
            relays: HashMap::new(),
            plain_conn_sender,
            listener,
            backoff_ticks,
            spawner,
        }
    }

    fn start_listen(&mut self, address: B, opt_friend: Option<PublicKey>) -> Result<(), ListenPoolError> {
        // TODO: Assert that we are not already listening to relay address?
        
        assert!(!self.relays.contains_key(&address));
        if let None = opt_friend {
            // No friend will try to connect on this address, but we defined it
            // as one of our local addresses:
            assert!(self.local_addresses.contains(&address));
        }
        
        let relay_friends = opt_friend
            .into_iter()
            .collect::<HashSet<_>>();

        // Fill in access_control:
        let mut access_control = AccessControlPk::new();

        for friend_public_key in &relay_friends {
            access_control.apply_op(AccessControlOp::Add(friend_public_key.clone()));
        }

        let (access_control_sender, mut connections_receiver) = 
            self.listener.clone().listen((address.clone(), access_control));
        // TODO: Do we need the listener.clone() here? Maybe Listen doesn't need to take ownership
        // over self?

        let mut c_plain_conn_sender = self.plain_conn_sender.clone();
        let send_fut = async move {
            let _ = await!(c_plain_conn_sender.send_all(&mut connections_receiver));
        };
        self.spawner.spawn(send_fut)
            .map_err(|_| ListenPoolError::SpawnError)?;

        let relay = Relay {
            friends: relay_friends,
            status: RelayStatus::Connected(access_control_sender),
        };

        self.relays.insert(address, relay);
        Ok(())
    }

    pub fn handle_config(&mut self, config: LpConfig<B>) -> Result<(), ListenPoolError> {
        match config {
            LpConfig::SetLocalAddresses(local_addresses) => {
                self.local_addresses = local_addresses
                    .iter()
                    .cloned()
                    .collect::<HashSet<_>>();

                // Remove relays that we don't need to listen to anymore:
                // This should disconnect from those relays automatically:
                self.relays.retain(|relay_address, relay| {
                    if !relay.friends.is_empty() {
                        return true;
                    }
                    local_addresses.contains(relay_address)
                });

                // Start listening to new relays if necessary:
                let mut new_addresses = Vec::new();
                for address in &self.local_addresses {
                    if !self.relays.contains_key(address) {
                        new_addresses.push(address.clone());
                    }
                }

                for address in new_addresses {
                    self.start_listen(address.clone(), None)?;
                }
            },
            LpConfig::UpdateFriend((friend_public_key, addresses)) => {
                for address in &addresses {
                    match self.relays.get_mut(address) {
                        Some(relay) => {
                            relay.friends.insert(friend_public_key.clone());
                        },
                        None => {
                            self.start_listen(address.clone(), Some(friend_public_key.clone()))?;
                        },
                    }
                }
            },
            LpConfig::RemoveFriend(friend_public_key) => {
                let local_addresses = self.local_addresses.clone();
                self.relays.retain(|relay_address, relay| {
                    // Update relay friends:
                    let _ = relay.friends.remove(&friend_public_key);
                    // We remove the relay connection if it was only required
                    // by the removed friend:
                    if !relay.friends.is_empty() {
                        return true;
                    }
                    local_addresses.contains(relay_address)
                });
            },
        };
        Ok(())
    }

    pub fn handle_incoming_encrypted_conn(&mut self,
                                      remote_public_key: PublicKey,
                                      enc_conn: RawConn) -> Result<(), ListenPoolError> {
        unimplemented!();
    }

    pub fn handle_relay_closed(&mut self,
                           address: B) -> Result<(), ListenPoolError> {

        unimplemented!();
    }

    pub fn handle_timer_tick(&mut self) -> Result<(), ListenPoolError> {
        unimplemented!();
    }
}


async fn listen_pool_loop<B,L,ET,TS,S>(incoming_config: mpsc::Receiver<LpConfig<B>>,
                                 listener: L,
                                 encrypt_transform: ET,
                                 max_concurrent_encrypt: usize,
                                 backoff_ticks: usize,
                                 timer_stream: TS,
                                 spawner: S) -> Result<(), ListenPoolError>
where
    B: Clone + Eq + Hash,
    L: Listener<Connection=(PublicKey, RawConn), 
        Config=AccessControlOpPk, Arg=(B, AccessControlPk)> + Clone + 'static,
    ET: FutTransform<Input=(PublicKey, RawConn), Output=Option<(PublicKey, RawConn)>> + Clone + Send + 'static,
    TS: Stream + Unpin,
    S: Spawn + Clone + Send + 'static,
{

    let (plain_conn_sender, incoming_encrypted_conn) = create_transform_pool(
                          encrypt_transform, 
                          max_concurrent_encrypt,
                          spawner.clone())
        .map_err(|_| ListenPoolError::CreateEncryptPoolError)?;

    let mut listen_pool = ListenPool::<B,L,S>::new(plain_conn_sender,
                                      listener,
                                      backoff_ticks,
                                      spawner);

    let incoming_encrypted_conn = incoming_encrypted_conn
        .map(|pk_enc_conn| LpEvent::IncomingEncryptedConn(pk_enc_conn));

    let incoming_config = incoming_config
        .map(|config| LpEvent::Config(config))
        .chain(stream::once(future::ready(LpEvent::ConfigClosed)));

    let timer_stream = timer_stream
        .map(|_| LpEvent::<B>::TimerTick)
        .chain(stream::once(future::ready(LpEvent::TimerClosed)));

    let mut incoming_events = incoming_encrypted_conn
        .select(incoming_config)
        .select(timer_stream);

    while let Some(event) = await!(incoming_events.next()) {
        match event {
            LpEvent::Config(config) => 
                listen_pool.handle_config(config)?,
            LpEvent::ConfigClosed => return Err(ListenPoolError::ConfigClosed),
            LpEvent::IncomingEncryptedConn((remote_public_key, enc_conn)) => 
                listen_pool.handle_incoming_encrypted_conn(remote_public_key, enc_conn)?,
            LpEvent::RelayClosed(address) => 
                listen_pool.handle_relay_closed(address)?,
            LpEvent::TimerTick => 
                listen_pool.handle_timer_tick()?,
            LpEvent::TimerClosed => return Err(ListenPoolError::TimerClosed),
        }
    }
    Ok(())
}

