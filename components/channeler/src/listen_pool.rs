use std::hash::Hash;
use std::marker::PhantomData;
use std::collections::{HashMap, HashSet};

use futures::{future, FutureExt, TryFutureExt, stream, Stream, StreamExt, SinkExt};
use futures::channel::mpsc;
use futures::task::{Spawn, SpawnExt};

use common::conn::{Listener, FutTransform};
use common::access_control::AccessControlOp;

use timer::TimerClient;

use crypto::identity::PublicKey;
use crate::types::{RawConn, AccessControlPk, AccessControlOpPk};
use crate::transform_pool::transform_pool_loop;

pub enum LpConfig<B> {
    SetLocalAddresses(Vec<B>),
    UpdateFriend((PublicKey, Vec<B>)),
    RemoveFriend(PublicKey),
}

/*

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
*/

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

struct Relay<P=PublicKey,ST=RelayStatus> {
    friends: HashSet<P>,
    status: ST,
}

struct ListenPoolState<B,P,ST> {
    relays: HashMap<B, Relay<P,ST>>,
    local_addresses: HashSet<B>,
}

impl<B,P,ST> ListenPoolState<B,P,ST> 
where
    B: Hash + Eq + Clone,
    P: Hash + Eq + Clone,
{
    pub fn new() -> Self {
        ListenPoolState {
            relays: HashMap::new(),
            local_addresses: HashSet::new(),
        }
    }

    pub fn set_local_addresses(&mut self, local_addresses: Vec<B>) 
        -> (HashSet<P>, Vec<B>) {
            // Should spawn_listen(address, relay_friends) for 
            // all addresses.

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


        // Local chosen relays should allow all friends to connect:
        let mut relay_friends = HashSet::new();
        for (_relay_address, relay) in &self.relays {
            for friend in &relay.friends {
                relay_friends.insert(friend.clone());
            }
        }

        (relay_friends, new_addresses)
    }

    pub fn update_friend(&mut self, 
                     friend_public_key: P,
                     addresses: Vec<B>)
                -> (Vec<B>, Vec<B>) {
                    // Relays to send AccessControlOp::Add
                    // Relays to spawn

        let mut relays_add = Vec::new();
        let mut relays_spawn = Vec::new();

        // We add the friend to all relevant relays (as specified by addresses),
        // but also to all our local addresses relays.
        let iter_addresses = addresses
            .into_iter()
            .chain(self.local_addresses.iter().cloned())
            .collect::<HashSet<_>>();

        for address in iter_addresses.iter() {
            match self.relays.get_mut(address) {
                Some(relay) => {
                    if relay.friends.contains(&friend_public_key) {
                        continue;
                    }
                    relays_add.push(address.clone());
                    relay.friends.insert(friend_public_key.clone());
                },
                None => {
                    relays_spawn.push(address.clone());
                },
            }
        }

        (relays_add, relays_spawn)
    }

    /// Outputs a set of relays to send AccessControlOp::Remove(friend_public_key)
    pub fn remove_friend(&mut self, friend_public_key: P) -> Vec<B> {
        let local_addresses = self.local_addresses.clone();

        let mut remove_friends = Vec::new();

        // Update access control:
        for (relay_address, relay) in &mut self.relays {
            if !relay.friends.contains(&friend_public_key) {
                continue;
            }
            remove_friends.push(relay_address.clone());
        }

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

        remove_friends
    }
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

    pub async fn handle_config(&mut self, config: LpConfig<B>) -> Result<(), ListenPoolError> {
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


                // Local chosen relays should allow all friends to connect:
                let mut relay_friends = HashSet::new();
                for (_relay_address, relay) in &self.relays {
                    for friend in &relay.friends {
                        relay_friends.insert(friend.clone());
                    }
                }

                for address in new_addresses {
                    let access_control_sender = self.spawn_listen(address.clone(), &relay_friends)?;
                    let relay = Relay {
                        friends: relay_friends.clone(),
                        status: RelayStatus::Connected(access_control_sender),
                    };
                    self.relays.insert(address, relay);
                }
            },
            LpConfig::UpdateFriend((friend_public_key, addresses)) => {
                // We add the friend to all relevant relays (as specified by addresses),
                // but also to all our local addresses relays.
                let iter_addresses = addresses
                    .into_iter()
                    .chain(self.local_addresses.iter().cloned())
                    .collect::<HashSet<_>>();

                for address in iter_addresses.iter() {
                    match self.relays.get_mut(address) {
                        Some(relay) => {
                            if relay.friends.contains(&friend_public_key) {
                                continue;
                            }
                            if let RelayStatus::Connected(access_control_sender) = &mut relay.status {
                                // TODO: Error checking here?
                                let _ = await!(access_control_sender.send(AccessControlOp::Add(friend_public_key.clone())));
                            }
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

                // Update access control:
                for (_relay_address, relay) in &mut self.relays {
                    if !relay.friends.contains(&friend_public_key) {
                        continue;
                    }
                    if let RelayStatus::Connected(access_control_sender) = &mut relay.status {
                        // TODO: Error checking here?
                        let _ = await!(access_control_sender.send(AccessControlOp::Remove(friend_public_key.clone())));
                    }
                }

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
async fn listen_pool_loop<B,L,TS,S>(incoming_config: mpsc::Receiver<LpConfig<B>>,
                                    outgoing_plain_conns: mpsc::Sender<(PublicKey, RawConn)>,
                                    listener: L,
                                    backoff_ticks: usize,
                                    timer_stream: TS,
                                    mut spawner: S,
                                    mut opt_event_sender: Option<mpsc::Sender<()>>) 
                        -> Result<(), ListenPoolError>
where
    B: Clone + Eq + Hash + Send + 'static,
    L: Listener<Connection=(PublicKey, RawConn), 
        Config=AccessControlOpPk, Arg=(B, AccessControlPk)> + Clone + 'static,
    TS: Stream + Unpin,
    S: Spawn + Clone + Send + 'static,
{
    
    let (relay_closed_sender, relay_closed_receiver) = mpsc::channel(0);

    let mut listen_pool = ListenPool::<B,L,S>::new(outgoing_plain_conns,
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
                await!(listen_pool.handle_config(config))?,
            LpEvent::ConfigClosed => return Err(ListenPoolError::ConfigClosed),
            LpEvent::RelayClosed(address) => 
                listen_pool.handle_relay_closed(address)?,
            LpEvent::TimerTick => 
                listen_pool.handle_timer_tick()?,
            LpEvent::TimerClosed => return Err(ListenPoolError::TimerClosed),
        };

        // Used for debugging:
        if let Some(ref mut event_sender) = opt_event_sender {
            let _ = await!(event_sender.send(()));
        }
    }
    Ok(())
}


#[derive(Clone)]
pub struct PoolListener<B,L,ET,S> {
    listener: L,
    encrypt_transform: ET,
    max_concurrent_encrypt: usize,
    backoff_ticks: usize,
    timer_client: TimerClient,
    spawner: S,
    phantom_b: PhantomData<B>,
}

impl<B,L,ET,S> PoolListener<B,L,ET,S> {
    #[allow(unused)]
    pub fn new(listener: L,
           encrypt_transform: ET,
           max_concurrent_encrypt: usize,
           backoff_ticks: usize,
           timer_client: TimerClient,
           spawner: S) -> Self {

        PoolListener {
            listener,
            encrypt_transform,
            max_concurrent_encrypt,
            backoff_ticks,
            timer_client,
            spawner,
            phantom_b: PhantomData,
        }
    }
}


impl<B,L,ET,S> Listener for PoolListener<B,L,ET,S> 
where
    B: Clone + Eq + Hash + Send + Sync + 'static,
    L: Listener<Connection=(PublicKey, RawConn), 
        Config=AccessControlOpPk, Arg=(B, AccessControlPk)> + Clone + Send + 'static,
    ET: FutTransform<Input=(PublicKey, RawConn), Output=Option<(PublicKey, RawConn)>> + Clone + Send + 'static,
    S: Spawn + Clone + Send + 'static,

{
    type Connection = (PublicKey, RawConn);
    type Config = LpConfig<B>;
    type Arg = ();

    fn listen(mut self, _arg: Self::Arg) -> (mpsc::Sender<Self::Config>, 
                             mpsc::Receiver<Self::Connection>) {

        let (config_sender, incoming_config) = mpsc::channel(0);
        let (outgoing_conns, incoming_conns) = mpsc::channel(0);

        let mut c_timer_client = self.timer_client.clone();
        let c_listener = self.listener.clone();
        let c_encrypt_transform = self.encrypt_transform.clone();
        let c_max_concurrent_encrypt = self.max_concurrent_encrypt.clone();
        let c_backoff_ticks = self.backoff_ticks.clone();
        let mut c_spawner = self.spawner.clone();

        // Connections encryptor:
        let (plain_conn_sender, incoming_plain_conn) = mpsc::channel(0);
        let enc_loop_fut = transform_pool_loop(incoming_plain_conn,
                            outgoing_conns,
                            c_encrypt_transform,
                            c_max_concurrent_encrypt,
                            c_spawner.clone())
            .map_err(|e| error!("transform_pool_loop: {:?}", e))
            .map(|_| ());

        if c_spawner.spawn(enc_loop_fut).is_err() {
            return (config_sender, incoming_conns)
        }

        let loop_fut = async move {
            let res_timer_stream = await!(c_timer_client.request_timer_stream());
            let timer_stream = match res_timer_stream {
                Ok(timer_stream) => timer_stream,
                Err(_) => {
                    error!("PoolListener::listen(): Failed to obtain timer stream!");
                    return;
                },
            };

            let res = await!(listen_pool_loop(
                incoming_config,
                plain_conn_sender,
                c_listener,
                c_backoff_ticks,
                timer_stream,
                c_spawner,
                None));
            
            if let Err(e) = res {
                error!("listen_pool_loop() error: {:?}", e);
            }
        };

        // If the spawn didn't work, incoming_conns will be closed (because outgoing_conns is
        // dropped) and the user of this listener will find out about it.
        let _ = self.spawner.spawn(loop_fut);

        (config_sender, incoming_conns)
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use futures::executor::ThreadPool;
    use futures::channel::mpsc;

    use crypto::identity::PUBLIC_KEY_LEN;

    use timer::{dummy_timer_multi_sender, TimerTick};
    use common::dummy_listener::DummyListener;

    async fn task_listen_pool_loop_set_local_addresses<S>(mut spawner: S) 
    where
        S: Spawn + Clone + Send + 'static,
    {
        // Create a mock time service:
        let (mut tick_sender_receiver, mut timer_client) 
            = dummy_timer_multi_sender(spawner.clone());
        let backoff_ticks = 2;

        let timer_stream = await!(timer_client.request_timer_stream()).unwrap();
        let _tick_sender = await!(tick_sender_receiver.next()).unwrap();

        let (mut config_sender, incoming_config) = mpsc::channel(0);
        let (outgoing_plain_conns, mut incoming_plain_conns) = mpsc::channel(0);

        let (listen_req_sender, mut listen_req_receiver) = mpsc::channel(0);
        let listener = DummyListener::new(listen_req_sender, spawner.clone());

        let (event_sender, mut event_receiver) = mpsc::channel(0);
        let fut_loop = listen_pool_loop::<u32,_,_,_>(incoming_config,
                         outgoing_plain_conns,
                         listener,
                         backoff_ticks,
                         timer_stream,
                         spawner.clone(),
                         Some(event_sender))
            .map_err(|e| error!("listen_pool_loop() error: {:?}", e))
            .map(|_| ());

        spawner.spawn(fut_loop).unwrap();

        let mut local_addresses = vec![0x0u32, 0x1u32];
        local_addresses.sort();

        await!(config_sender.send(LpConfig::SetLocalAddresses(local_addresses.clone()))).unwrap();
        await!(event_receiver.next()).unwrap();

        let mut observed_addresses = Vec::new();
        let mut listen_req0 = await!(listen_req_receiver.next()).unwrap();
        let (ref relay_address0, _) = listen_req0.arg;
        observed_addresses.push(relay_address0.clone());

        let pk_b = PublicKey::from(&[0xbb; PUBLIC_KEY_LEN]);

        // Send a few connections:
        for _ in 0 .. 5usize {
            let (_local_sender, remote_receiver) = mpsc::channel(0);
            let (remote_sender, _local_receiver) = mpsc::channel(0);
            await!(listen_req0.conn_sender.send(
                    (pk_b.clone(), (remote_sender, remote_receiver)))).unwrap();

            let (pk, _conn) = await!(incoming_plain_conns.next()).unwrap();
            assert_eq!(pk, pk_b);
        }

        let mut listen_req1 = await!(listen_req_receiver.next()).unwrap();
        let (ref relay_address1, _) = listen_req1.arg;
        observed_addresses.push(relay_address1.clone());

        observed_addresses.sort();
        assert_eq!(local_addresses, observed_addresses);


        // Reduce the set of local addresses to only contain 0x1u32:
        await!(config_sender.send(LpConfig::SetLocalAddresses(vec![0x1u32]))).unwrap();
        await!(event_receiver.next()).unwrap();

        // The 0x0u32 listener should be closed:
        if *relay_address0 == 0x0u32 {
            assert!(await!(listen_req0.config_receiver.next()).is_none());
        } else {
            assert!(await!(listen_req1.config_receiver.next()).is_none());
        }
    }

    #[test]
    fn test_listen_pool_loop_set_local_addresses() {
        let mut thread_pool = ThreadPool::new().unwrap();
        thread_pool.run(task_listen_pool_loop_set_local_addresses(thread_pool.clone()));
    }


    // ----------------------------------------------------------------
    // ----------------------------------------------------------------
    
    async fn task_listen_pool_loop_backoff_ticks<S>(mut spawner: S) 
    where
        S: Spawn + Clone + Send + 'static,
    {
        // Create a mock time service:
        let (mut tick_sender_receiver, mut timer_client) 
            = dummy_timer_multi_sender(spawner.clone());
        let backoff_ticks = 2;

        let timer_stream = await!(timer_client.request_timer_stream()).unwrap();
        let mut tick_sender = await!(tick_sender_receiver.next()).unwrap();

        let (mut config_sender, incoming_config) = mpsc::channel(0);
        let (outgoing_plain_conns, _incoming_plain_conns) = mpsc::channel(0);

        let (listen_req_sender, mut listen_req_receiver) = mpsc::channel(0);
        let listener = DummyListener::new(listen_req_sender, spawner.clone());

        let (event_sender, mut event_receiver) = mpsc::channel(0);
        let fut_loop = listen_pool_loop::<u32,_,_,_>(incoming_config,
                         outgoing_plain_conns,
                         listener,
                         backoff_ticks,
                         timer_stream,
                         spawner.clone(),
                         Some(event_sender))
            .map_err(|e| error!("listen_pool_loop() error: {:?}", e))
            .map(|_| ());

        spawner.spawn(fut_loop).unwrap();

        await!(config_sender.send(LpConfig::SetLocalAddresses(vec![0x0u32]))).unwrap();
        await!(event_receiver.next()).unwrap();

        for _ in 0 .. 5 {
            let listen_req = await!(listen_req_receiver.next()).unwrap();
            let (ref relay_address, _) = listen_req.arg;
            assert_eq!(*relay_address, 0);

            // Simulate closing of the listener:
            drop(listen_req);
            await!(event_receiver.next()).unwrap();

            // Wait until backoff_ticks time passes:
            for _ in 0 .. backoff_ticks {
                await!(tick_sender.send(TimerTick)).unwrap();
                await!(event_receiver.next()).unwrap();
            }
        }

        let listen_req = await!(listen_req_receiver.next()).unwrap();
        let (ref relay_address, _) = listen_req.arg;
        assert_eq!(*relay_address, 0);
    }

    #[test]
    fn test_listen_pool_loop_backoff_ticks() {
        let mut thread_pool = ThreadPool::new().unwrap();
        thread_pool.run(task_listen_pool_loop_backoff_ticks(thread_pool.clone()));
    }
}
