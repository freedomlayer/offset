use std::hash::Hash;
use std::collections::{HashMap, HashSet};
use futures::{future, FutureExt, TryFutureExt, stream, Stream, StreamExt, Sink, SinkExt};
use futures::channel::mpsc;
use futures::task::{Spawn, SpawnExt};

use common::conn::{Listener, FutTransform};
use common::access_control::{AccessControlOp, AccessControl};

use crypto::identity::PublicKey;
use crate::types::{RawConn, AccessControlPk, AccessControlOpPk};
use crate::transform_pool::transform_pool_loop;

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
    relays: HashMap<B, Relay>,
    plain_conn_sender: mpsc::Sender<(PublicKey, RawConn)>,
    relay_closed_sender: mpsc::Sender<B>,
    listener: L,
    backoff_ticks: usize,
    spawner: S,
}

impl<B,L,S> ListenPool<B,L,S> 
where
    B: Hash + Eq + Clone + Send + 'static,
    L: Listener<Connection=(PublicKey, RawConn), 
        Config=AccessControlOpPk, Arg=(B, AccessControlPk)> + Clone + 'static,
    S: Spawn + Clone,
{
    pub fn new(plain_conn_sender: mpsc::Sender<(PublicKey, RawConn)>,
               relay_closed_sender: mpsc::Sender<B>,
               listener: L,
               backoff_ticks: usize,
               spawner: S) -> Self {

        ListenPool {
            local_addresses: HashSet::new(),
            relays: HashMap::new(),
            plain_conn_sender,
            relay_closed_sender,
            listener,
            backoff_ticks,
            spawner,
        }
    }

    fn spawn_listen(&self, address: B, relay_friends: &HashSet<PublicKey>) 
        -> Result<mpsc::Sender<AccessControlOpPk>, ListenPoolError> {

        // Fill in access_control:
        let mut access_control = AccessControlPk::new();

        for friend_public_key in relay_friends {
            access_control.apply_op(AccessControlOp::Add(friend_public_key.clone()));
        }

        let (access_control_sender, mut connections_receiver) = 
            self.listener.clone().listen((address.clone(), access_control));
        // TODO: Do we need the listener.clone() here? Maybe Listen doesn't need to take ownership
        // over self?

        let mut c_plain_conn_sender = self.plain_conn_sender.clone();
        let mut c_relay_closed_sender = self.relay_closed_sender.clone();
        let send_fut = async move {
            let _ = await!(c_plain_conn_sender.send_all(&mut connections_receiver));
            // Notify that this listener was closed:
            let _ = await!(c_relay_closed_sender.send(address));
        };
        self.spawner.clone().spawn(send_fut)
            .map_err(|_| ListenPoolError::SpawnError)?;

        Ok(access_control_sender)
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
                    let relay_friends = HashSet::new();
                    let access_control_sender = self.spawn_listen(address.clone(), &relay_friends)?;
                    let relay = Relay {
                        friends: relay_friends,
                        status: RelayStatus::Connected(access_control_sender),
                    };
                    self.relays.insert(address, relay);
                }
            },
            LpConfig::UpdateFriend((friend_public_key, addresses)) => {
                for address in &addresses {
                    match self.relays.get_mut(address) {
                        Some(relay) => {
                            relay.friends.insert(friend_public_key.clone());
                        },
                        None => {
                            let mut relay_friends = HashSet::new();
                            relay_friends.insert(friend_public_key.clone());
                            let access_control_sender = self.spawn_listen(address.clone(), &relay_friends)?;
                            let relay = Relay {
                                friends: relay_friends,
                                status: RelayStatus::Connected(access_control_sender),
                            };
                            self.relays.insert(address.clone(), relay);
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

    pub fn handle_relay_closed(&mut self,
                           address: B) -> Result<(), ListenPoolError> {

        let relay = match self.relays.get_mut(&address) {
            Some(relay) => relay,
            None => return Ok(()), // TODO: Could this happen?
        };

        relay.status = RelayStatus::Waiting(self.backoff_ticks);
        Ok(())
    }

    pub fn handle_timer_tick(&mut self) -> Result<(), ListenPoolError> {
        let mut spawn_addresses = Vec::new();
        for (address, relay) in &mut self.relays {
            match &mut relay.status {
                RelayStatus::Waiting(ref mut remaining_ticks) => {
                    *remaining_ticks = (*remaining_ticks).saturating_sub(1);
                    if *remaining_ticks > 0 {
                        continue;
                    }
                    spawn_addresses.push(address.clone());
                },
                RelayStatus::Connected(_access_control_sender) => {}, // Nothing to do
            }
        }

        // Reconnect to relays for which enough time has passed:
        for address in spawn_addresses {
            let relay = self.relays.get(&address).unwrap();
            let access_control_sender = self.spawn_listen(address.clone(), &relay.friends)?;

            let relay = self.relays.get_mut(&address).unwrap();
            relay.status = RelayStatus::Connected(access_control_sender);
        }
        Ok(())
    }
}


#[allow(unused)]
async fn listen_pool_loop<B,L,ET,TS,S>(incoming_config: mpsc::Receiver<LpConfig<B>>,
                                       outgoing_conns: mpsc::Sender<(PublicKey, RawConn)>,
                                       listener: L,
                                       encrypt_transform: ET,
                                       max_concurrent_encrypt: usize,
                                       backoff_ticks: usize,
                                       timer_stream: TS,
                                       mut spawner: S) -> Result<(), ListenPoolError>
where
    B: Clone + Eq + Hash + Send + 'static,
    L: Listener<Connection=(PublicKey, RawConn), 
        Config=AccessControlOpPk, Arg=(B, AccessControlPk)> + Clone + 'static,
    ET: FutTransform<Input=(PublicKey, RawConn), Output=Option<(PublicKey, RawConn)>> + Clone + Send + 'static,
    TS: Stream + Unpin,
    S: Spawn + Clone + Send + 'static,
{
    
    let (plain_conn_sender, incoming_plain_conn) = mpsc::channel(0);
    let enc_loop_fut = transform_pool_loop(incoming_plain_conn,
                        outgoing_conns,
                        encrypt_transform,
                        max_concurrent_encrypt,
                        spawner.clone())
        .map_err(|e| error!("transform_pool_loop: {:?}", e))
        .map(|_| ());

    spawner.spawn(enc_loop_fut)
        .map_err(|_| ListenPoolError::SpawnError)?;
    
    let (relay_closed_sender, relay_closed_receiver) = mpsc::channel(0);

    let mut listen_pool = ListenPool::<B,L,S>::new(plain_conn_sender,
                                                   relay_closed_sender,
                                                   listener,
                                                   backoff_ticks,
                                                   spawner);

    let incoming_relay_closed = relay_closed_receiver
        .map(|address| LpEvent::RelayClosed(address));

    let incoming_config = incoming_config
        .map(|config| LpEvent::Config(config))
        .chain(stream::once(future::ready(LpEvent::ConfigClosed)));

    let timer_stream = timer_stream
        .map(|_| LpEvent::<B>::TimerTick)
        .chain(stream::once(future::ready(LpEvent::TimerClosed)));

    let mut incoming_events = incoming_relay_closed
        .select(incoming_config)
        .select(timer_stream);

    while let Some(event) = await!(incoming_events.next()) {
        match event {
            LpEvent::Config(config) => 
                listen_pool.handle_config(config)?,
            LpEvent::ConfigClosed => return Err(ListenPoolError::ConfigClosed),
            LpEvent::RelayClosed(address) => 
                listen_pool.handle_relay_closed(address)?,
            LpEvent::TimerTick => 
                listen_pool.handle_timer_tick()?,
            LpEvent::TimerClosed => return Err(ListenPoolError::TimerClosed),
        }
    }
    Ok(())
}

