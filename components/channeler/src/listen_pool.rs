use std::collections::HashSet;
use std::fmt::Debug;
use std::hash::Hash;
use std::marker::PhantomData;

use futures::channel::mpsc;
use futures::task::{Spawn, SpawnExt};
use futures::{future, stream, FutureExt, SinkExt, Stream, StreamExt, TryFutureExt};

use common::access_control::AccessControlOp;
use common::conn::{
    BoxStream, ConnPairVec, FutListenerClient, FutTransform, Listener, ListenerClient,
};
use common::select_streams::select_streams;
use common::transform_pool::transform_pool_loop;

use timer::TimerClient;

use proto::crypto::PublicKey;

use crate::listen_pool_state::{ListenPoolState, Relay};
use crate::types::{AccessControlOpPk, AccessControlPk};

#[derive(Debug, PartialEq, Eq)]
pub enum LpConfig<RA> {
    SetLocalAddresses(Vec<RA>),
    UpdateFriend((PublicKey, Vec<RA>)),
    RemoveFriend(PublicKey),
}

#[derive(Debug)]
enum ListenPoolError {
    SpawnError,
}

enum LpEvent<RA> {
    Config(LpConfig<RA>),
    ConfigClosed,
    RelayClosed(RA),
    TimerTick,
    TimerClosed,
}

enum RelayStatus {
    Waiting(usize), // ticks left to start listening again
    Connected(mpsc::Sender<AccessControlOpPk>),
}

struct ListenPool<RA, L, S> {
    state: ListenPoolState<RA, PublicKey, RelayStatus>,
    plain_conn_sender: mpsc::Sender<(PublicKey, ConnPairVec)>,
    relay_closed_sender: mpsc::Sender<RA>,
    listener: L,
    backoff_ticks: usize,
    spawner: S,
}

impl<RA, L, S> ListenPool<RA, L, S>
where
    RA: Hash + Eq + Clone + Send + Debug + 'static,
    L: Listener<
            Connection = (PublicKey, ConnPairVec),
            Config = AccessControlOpPk,
            Arg = (RA, AccessControlPk),
        > + Clone
        + 'static,
    S: Spawn + Clone + Send + 'static,
{
    pub fn new(
        plain_conn_sender: mpsc::Sender<(PublicKey, ConnPairVec)>,
        relay_closed_sender: mpsc::Sender<RA>,
        listener: L,
        backoff_ticks: usize,
        spawner: S,
    ) -> Self {
        ListenPool {
            state: ListenPoolState::new(),
            plain_conn_sender,
            relay_closed_sender,
            listener,
            backoff_ticks,
            spawner,
        }
    }

    fn spawn_listen(
        &self,
        address: RA,
        relay_friends: &HashSet<PublicKey>,
    ) -> Result<mpsc::Sender<AccessControlOpPk>, ListenPoolError> {
        // Fill in access_control:
        let mut access_control = AccessControlPk::new();

        for friend_public_key in relay_friends {
            access_control.apply_op(AccessControlOp::Add(friend_public_key.clone()));
        }

        let (user_access_control_sender, user_incoming_access_control) = mpsc::channel(0);

        let mut c_plain_conn_sender = self.plain_conn_sender.clone();
        let mut c_relay_closed_sender = self.relay_closed_sender.clone();
        let c_spawner = self.spawner.clone();

        let listen_fut = self
            .listener
            .clone()
            .listen((address.clone(), access_control));

        let send_fut = async move {
            // We use an extra async block here to always be sure to notify a listener was closed
            // (See below).
            let _ = async move {
                // Start listening to relay:
                let ListenerClient {
                    config_sender: access_control_sender,
                    conn_receiver: connections_receiver,
                } = listen_fut.await.map_err(|_| ())?;

                // Forward all user control messages:
                // Should be dropped when this future is dropped.
                let _control_forward_handle = c_spawner
                    .spawn_with_handle(
                        user_incoming_access_control
                            .map(Ok)
                            .forward(access_control_sender),
                    )
                    .map_err(|_| ())?;

                // Forward incoming connections to user:
                let _ = c_plain_conn_sender
                    .send_all(&mut connections_receiver.map(Ok))
                    .await;

                Ok::<_, ()>(())
            }
            .await;

            // Finally, notify that this listener was closed:
            let _ = c_relay_closed_sender.send(address).await;
        };

        self.spawner
            .clone()
            .spawn(send_fut)
            .map_err(|_| ListenPoolError::SpawnError)?;

        Ok(user_access_control_sender)
    }

    pub async fn handle_config(&mut self, config: LpConfig<RA>) -> Result<(), ListenPoolError> {
        match config {
            LpConfig::SetLocalAddresses(local_addresses) => {
                let (relay_friends, addresses) = self.state.set_local_addresses(local_addresses);
                for address in addresses {
                    let access_control_sender =
                        self.spawn_listen(address.clone(), &relay_friends)?;
                    let relay = Relay {
                        friends: relay_friends.clone(),
                        status: RelayStatus::Connected(access_control_sender),
                    };
                    self.state.relays.insert(address, relay);
                }
            }
            LpConfig::UpdateFriend((friend_public_key, addresses)) => {
                let (relays_add, relays_remove, relays_spawn) = self
                    .state
                    .update_friend(friend_public_key.clone(), addresses);

                for address in relays_add {
                    if let Some(relay) = self.state.relays.get_mut(&address) {
                        if let RelayStatus::Connected(access_control_sender) = &mut relay.status {
                            // TODO: Error checking here?
                            let _ = access_control_sender
                                .send(AccessControlOp::Add(friend_public_key.clone()))
                                .await;
                        }
                    }
                }

                for address in relays_remove {
                    if let Some(relay) = self.state.relays.get_mut(&address) {
                        if let RelayStatus::Connected(access_control_sender) = &mut relay.status {
                            // TODO: Error checking here?
                            let _ = access_control_sender
                                .send(AccessControlOp::Remove(friend_public_key.clone()))
                                .await;
                        }
                    }
                }

                for address in relays_spawn {
                    let mut relay_friends = HashSet::new();
                    relay_friends.insert(friend_public_key.clone());
                    let access_control_sender =
                        self.spawn_listen(address.clone(), &relay_friends)?;
                    let relay = Relay {
                        friends: relay_friends,
                        status: RelayStatus::Connected(access_control_sender),
                    };
                    self.state.relays.insert(address.clone(), relay);
                }
            }
            LpConfig::RemoveFriend(friend_public_key) => {
                let remove_relays = self.state.remove_friend(&friend_public_key);

                for address in remove_relays {
                    if let Some(relay) = self.state.relays.get_mut(&address) {
                        if let RelayStatus::Connected(access_control_sender) = &mut relay.status {
                            // TODO: Error checking here?
                            let _ = access_control_sender
                                .send(AccessControlOp::Remove(friend_public_key.clone()))
                                .await;
                        }
                    }
                }
            }
        };
        Ok(())
    }

    pub fn handle_relay_closed(&mut self, address: RA) -> Result<(), ListenPoolError> {
        let relay = match self.state.relays.get_mut(&address) {
            Some(relay) => relay,
            None => return Ok(()), // TODO: Could this happen?
        };

        relay.status = RelayStatus::Waiting(self.backoff_ticks);
        Ok(())
    }

    pub fn handle_timer_tick(&mut self) -> Result<(), ListenPoolError> {
        let mut spawn_addresses = Vec::new();
        for (address, relay) in &mut self.state.relays {
            match &mut relay.status {
                RelayStatus::Waiting(ref mut remaining_ticks) => {
                    *remaining_ticks = (*remaining_ticks).saturating_sub(1);
                    if *remaining_ticks > 0 {
                        continue;
                    }
                    spawn_addresses.push(address.clone());
                }
                RelayStatus::Connected(_access_control_sender) => {} // Nothing to do
            }
        }

        // Reconnect to relays for which enough time has passed:
        for address in spawn_addresses {
            let relay = self.state.relays.get(&address).unwrap();
            let access_control_sender = self.spawn_listen(address.clone(), &relay.friends)?;

            let relay = self.state.relays.get_mut(&address).unwrap();
            relay.status = RelayStatus::Connected(access_control_sender);
        }
        Ok(())
    }
}

async fn listen_pool_loop<RA, L, TS, S>(
    incoming_config: mpsc::Receiver<LpConfig<RA>>,
    outgoing_plain_conns: mpsc::Sender<(PublicKey, ConnPairVec)>,
    listener: L,
    backoff_ticks: usize,
    timer_stream: TS,
    spawner: S,
    mut opt_event_sender: Option<mpsc::Sender<()>>,
) -> Result<(), ListenPoolError>
where
    RA: Clone + Eq + Hash + Send + Debug + 'static,
    L: Listener<
            Connection = (PublicKey, ConnPairVec),
            Config = AccessControlOpPk,
            Arg = (RA, AccessControlPk),
        > + Clone
        + 'static,
    TS: Stream + Unpin + Send,
    S: Spawn + Clone + Send + 'static,
{
    let (relay_closed_sender, relay_closed_receiver) = mpsc::channel(0);

    let mut listen_pool = ListenPool::<RA, L, S>::new(
        outgoing_plain_conns,
        relay_closed_sender,
        listener,
        backoff_ticks,
        spawner,
    );

    let incoming_relay_closed = relay_closed_receiver.map(LpEvent::RelayClosed);

    let incoming_config = incoming_config
        .map(LpEvent::Config)
        .chain(stream::once(future::ready(LpEvent::ConfigClosed)));

    let timer_stream = timer_stream
        .map(|_| LpEvent::<RA>::TimerTick)
        .chain(stream::once(future::ready(LpEvent::TimerClosed)));

    let mut incoming_events = select_streams![incoming_relay_closed, incoming_config, timer_stream];

    while let Some(event) = incoming_events.next().await {
        match event {
            LpEvent::Config(config) => listen_pool.handle_config(config).await?,
            LpEvent::ConfigClosed => break,
            LpEvent::RelayClosed(address) => listen_pool.handle_relay_closed(address)?,
            LpEvent::TimerTick => listen_pool.handle_timer_tick()?,
            LpEvent::TimerClosed => break,
        };

        // Used for debugging:
        if let Some(ref mut event_sender) = opt_event_sender {
            let _ = event_sender.send(()).await;
        }
    }
    Ok(())
}

/// PoolListener Manages incoming connections through relays.
/// Can be configured by sending config messages.
#[derive(Clone)]
pub struct PoolListener<RA, L, ET, S> {
    listener: L,
    encrypt_transform: ET,
    max_concurrent_encrypt: usize,
    backoff_ticks: usize,
    timer_client: TimerClient,
    spawner: S,
    phantom_b: PhantomData<RA>,
}

impl<RA, L, ET, S> PoolListener<RA, L, ET, S> {
    pub fn new(
        listener: L,
        encrypt_transform: ET,
        max_concurrent_encrypt: usize,
        backoff_ticks: usize,
        timer_client: TimerClient,
        spawner: S,
    ) -> Self {
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

#[derive(Debug)]
pub struct PoolListenerError;

impl<RA, L, ET, S> Listener for PoolListener<RA, L, ET, S>
where
    RA: Clone + Eq + Hash + Send + Sync + Debug + 'static,
    L: Listener<
            Connection = (PublicKey, ConnPairVec),
            Config = AccessControlOpPk,
            Arg = (RA, AccessControlPk),
        > + Clone
        + Send
        + 'static,
    ET: FutTransform<Input = (PublicKey, ConnPairVec), Output = Option<(PublicKey, ConnPairVec)>>
        + Clone
        + Send
        + 'static,
    S: Spawn + Clone + Send + 'static,
{
    type Connection = (PublicKey, ConnPairVec);
    type Config = LpConfig<RA>;
    type Error = PoolListenerError;
    type Arg = ();

    fn listen(
        self,
        _arg: Self::Arg,
    ) -> FutListenerClient<Self::Config, Self::Connection, Self::Error> {
        let (config_sender, incoming_config) = mpsc::channel(0);
        let (outgoing_conns, incoming_conns) = mpsc::channel(0);

        let mut c_timer_client = self.timer_client.clone();
        let c_listener = self.listener.clone();
        let c_encrypt_transform = self.encrypt_transform.clone();
        let c_max_concurrent_encrypt = self.max_concurrent_encrypt;
        let c_backoff_ticks = self.backoff_ticks;
        let c_spawner = self.spawner.clone();

        // Connections encryptor:
        let (plain_conn_sender, incoming_plain_conn) = mpsc::channel(0);
        let enc_loop_fut = transform_pool_loop(
            incoming_plain_conn,
            outgoing_conns,
            c_encrypt_transform,
            c_max_concurrent_encrypt,
        )
        .map_err(|e| error!("transform_pool_loop: {:?}", e))
        .map(|_| ());

        if c_spawner.spawn(enc_loop_fut).is_err() {
            return Box::pin(future::ready(Err(PoolListenerError)));
        }

        let loop_fut = async move {
            let res_timer_stream = c_timer_client
                .request_timer_stream("PoolListener::listen".to_owned())
                .await;
            let timer_stream = match res_timer_stream {
                Ok(timer_stream) => timer_stream,
                Err(_) => {
                    error!("PoolListener::listen(): Failed to obtain timer stream!");
                    return;
                }
            };

            let res = listen_pool_loop(
                incoming_config,
                plain_conn_sender,
                c_listener,
                c_backoff_ticks,
                timer_stream,
                c_spawner,
                None,
            )
            .await;

            if let Err(e) = res {
                error!("listen_pool_loop() error: {:?}", e);
            }
        };

        if self.spawner.spawn(loop_fut).is_err() {
            // Fail early if spawning failed
            return Box::pin(future::ready(Err(PoolListenerError)));
        }

        Box::pin(future::ready(Ok(ListenerClient {
            config_sender,
            conn_receiver: incoming_conns,
        })))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::channel::mpsc;
    use futures::executor::{block_on, ThreadPool};

    use common::dummy_listener::DummyListener;
    use timer::{dummy_timer_multi_sender, TimerTick};

    async fn task_listen_pool_loop_set_local_addresses<S>(spawner: S)
    where
        S: Spawn + Clone + Send + 'static,
    {
        // Create a mock time service:
        let (mut tick_sender_receiver, mut timer_client) =
            dummy_timer_multi_sender(spawner.clone());
        let backoff_ticks = 2;

        let timer_stream = timer_client
            .request_timer_stream("task_listen_pool_loop_set_local_addresses".to_owned())
            .await
            .unwrap();
        let _tick_sender = tick_sender_receiver.next().await.unwrap();

        let (mut config_sender, incoming_config) = mpsc::channel(0);
        let (outgoing_plain_conns, mut incoming_plain_conns) = mpsc::channel(0);

        let (listen_req_sender, mut listen_req_receiver) = mpsc::channel(0);
        let listener = DummyListener::new(listen_req_sender);

        let (event_sender, mut event_receiver) = mpsc::channel(0);
        let fut_loop = listen_pool_loop::<u32, _, _, _>(
            incoming_config,
            outgoing_plain_conns,
            listener,
            backoff_ticks,
            timer_stream,
            spawner.clone(),
            Some(event_sender),
        )
        .map_err(|e| error!("listen_pool_loop() error: {:?}", e))
        .map(|_| ());

        spawner.spawn(fut_loop).unwrap();

        let mut local_addresses = vec![0x0u32, 0x1u32];
        local_addresses.sort();

        config_sender
            .send(LpConfig::SetLocalAddresses(local_addresses.clone()))
            .await
            .unwrap();
        event_receiver.next().await.unwrap();

        let mut observed_addresses = Vec::new();
        let mut listen_req0 = listen_req_receiver.next().await.unwrap();
        let (ref relay_address0, _) = listen_req0.arg;
        observed_addresses.push(relay_address0.clone());

        let pk_b = PublicKey::from(&[0xbb; PublicKey::len()]);

        // Send a few connections:
        for _ in 0..5usize {
            let (_local_sender, remote_receiver) = mpsc::channel(0);
            let (remote_sender, _local_receiver) = mpsc::channel(0);
            listen_req0
                .conn_sender
                .send((
                    pk_b.clone(),
                    ConnPairVec::from_raw(remote_sender, remote_receiver),
                ))
                .await
                .unwrap();

            let (pk, _conn) = incoming_plain_conns.next().await.unwrap();
            assert_eq!(pk, pk_b);
        }

        let mut listen_req1 = listen_req_receiver.next().await.unwrap();
        let (ref relay_address1, _) = listen_req1.arg;
        observed_addresses.push(relay_address1.clone());

        observed_addresses.sort();
        assert_eq!(local_addresses, observed_addresses);

        // Reduce the set of local addresses to only contain 0x1u32:
        config_sender
            .send(LpConfig::SetLocalAddresses(vec![0x1u32]))
            .await
            .unwrap();
        event_receiver.next().await.unwrap();

        // The 0x0u32 listener should be closed:
        if *relay_address0 == 0x0u32 {
            assert!(listen_req0.config_receiver.next().await.is_none());
        } else {
            assert!(listen_req1.config_receiver.next().await.is_none());
        }
    }

    #[test]
    fn test_listen_pool_loop_set_local_addresses() {
        let thread_pool = ThreadPool::new().unwrap();
        block_on(task_listen_pool_loop_set_local_addresses(
            thread_pool.clone(),
        ));
    }

    // ----------------------------------------------------------------
    // ----------------------------------------------------------------

    async fn task_listen_pool_loop_backoff_ticks<S>(spawner: S)
    where
        S: Spawn + Clone + Send + 'static,
    {
        // Create a mock time service:
        let (mut tick_sender_receiver, mut timer_client) =
            dummy_timer_multi_sender(spawner.clone());
        let backoff_ticks = 2;

        let timer_stream = timer_client
            .request_timer_stream("task_listen_pool_loop_backoff_ticks".to_owned())
            .await
            .unwrap();
        let mut tick_sender = tick_sender_receiver.next().await.unwrap();

        let (mut config_sender, incoming_config) = mpsc::channel(0);
        let (outgoing_plain_conns, _incoming_plain_conns) = mpsc::channel(0);

        let (listen_req_sender, mut listen_req_receiver) = mpsc::channel(0);
        let listener = DummyListener::new(listen_req_sender);

        let (event_sender, mut event_receiver) = mpsc::channel(0);
        let fut_loop = listen_pool_loop::<u32, _, _, _>(
            incoming_config,
            outgoing_plain_conns,
            listener,
            backoff_ticks,
            timer_stream,
            spawner.clone(),
            Some(event_sender),
        )
        .map_err(|e| error!("listen_pool_loop() error: {:?}", e))
        .map(|_| ());

        spawner.spawn(fut_loop).unwrap();

        config_sender
            .send(LpConfig::SetLocalAddresses(vec![0x0u32]))
            .await
            .unwrap();
        event_receiver.next().await.unwrap();

        for _ in 0..5 {
            let listen_req = listen_req_receiver.next().await.unwrap();
            let (ref relay_address, _) = listen_req.arg;
            assert_eq!(*relay_address, 0);

            // Simulate closing of the listener:
            drop(listen_req);
            event_receiver.next().await.unwrap();

            // Wait until backoff_ticks time passes:
            for _ in 0..backoff_ticks {
                tick_sender.send(TimerTick).await.unwrap();
                event_receiver.next().await.unwrap();
            }
        }

        let listen_req = listen_req_receiver.next().await.unwrap();
        let (ref relay_address, _) = listen_req.arg;
        assert_eq!(*relay_address, 0);
    }

    #[test]
    fn test_listen_pool_loop_backoff_ticks() {
        let thread_pool = ThreadPool::new().unwrap();
        block_on(task_listen_pool_loop_backoff_ticks(thread_pool.clone()));
    }

    // ------------------------------------------------------
    // ------------------------------------------------------

    async fn task_listen_pool_loop_update_remove_friend<S>(spawner: S)
    where
        S: Spawn + Clone + Send + 'static,
    {
        // Create a mock time service:
        let (mut tick_sender_receiver, mut timer_client) =
            dummy_timer_multi_sender(spawner.clone());
        let backoff_ticks = 2;

        let timer_stream = timer_client
            .request_timer_stream("task_listen_pool_loop_update_remove_friend".to_owned())
            .await
            .unwrap();
        let _tick_sender = tick_sender_receiver.next().await.unwrap();

        let (mut config_sender, incoming_config) = mpsc::channel(1);
        let (outgoing_plain_conns, _incoming_plain_conns) = mpsc::channel(0);

        let (listen_req_sender, mut listen_req_receiver) = mpsc::channel(0);
        let listener = DummyListener::new(listen_req_sender);

        let (event_sender, mut event_receiver) = mpsc::channel(0);
        let fut_loop = listen_pool_loop::<u32, _, _, _>(
            incoming_config,
            outgoing_plain_conns,
            listener,
            backoff_ticks,
            timer_stream,
            spawner.clone(),
            Some(event_sender),
        )
        .map_err(|e| error!("listen_pool_loop() error: {:?}", e))
        .map(|_| ());

        spawner.spawn(fut_loop).unwrap();

        config_sender
            .send(LpConfig::SetLocalAddresses(vec![0x0u32]))
            .await
            .unwrap();
        event_receiver.next().await.unwrap();

        let mut listen_req0 = listen_req_receiver.next().await.unwrap();
        let (ref relay_address0, _) = listen_req0.arg;
        assert_eq!(*relay_address0, 0x0u32);

        let pk_b = PublicKey::from(&[0xbb; PublicKey::len()]);

        config_sender
            .send(LpConfig::UpdateFriend((pk_b.clone(), vec![0x1u32])))
            .await
            .unwrap();
        event_receiver.next().await.unwrap();

        let mut listen_req1 = listen_req_receiver.next().await.unwrap();
        let (ref relay_address1, _) = listen_req1.arg;
        assert_eq!(*relay_address1, 0x1u32);

        let config0 = listen_req0.config_receiver.next().await.unwrap();
        match config0 {
            AccessControlOp::Add(pk) => assert_eq!(pk, pk_b),
            _ => unreachable!(),
        };

        config_sender
            .send(LpConfig::UpdateFriend((pk_b.clone(), vec![0x2u32])))
            .await
            .unwrap();
        event_receiver.next().await.unwrap();

        // Connection to relay 1u32 should be closed:
        assert!(listen_req1.config_receiver.next().await.is_none());
        drop(listen_req1);

        // Connection to relay 2u32 is opened:
        let mut listen_req2 = listen_req_receiver.next().await.unwrap();
        let (ref relay_address2, _) = listen_req2.arg;
        assert_eq!(*relay_address2, 0x2u32);

        let pk_c = PublicKey::from(&[0xcc; PublicKey::len()]);

        config_sender
            .send(LpConfig::UpdateFriend((pk_c.clone(), vec![0x2u32, 0x3u32])))
            .await
            .unwrap();
        event_receiver.next().await.unwrap();

        // Connection to relay 3u32 is opened:
        let mut listen_req3 = listen_req_receiver.next().await.unwrap();
        let (ref relay_address3, _) = listen_req3.arg;
        assert_eq!(*relay_address3, 0x3u32);

        for listen_req in &mut [&mut listen_req0, &mut listen_req2] {
            let config = listen_req.config_receiver.next().await.unwrap();
            match config {
                AccessControlOp::Add(pk) => assert_eq!(pk, pk_c),
                _ => unreachable!(),
            };
        }

        config_sender
            .send(LpConfig::RemoveFriend(pk_c.clone()))
            .await
            .unwrap();
        event_receiver.next().await.unwrap();

        // Connection to relay 3u32 should be closed:
        assert!(listen_req3.config_receiver.next().await.is_none());
        drop(listen_req3);

        for listen_req in &mut [&mut listen_req0, &mut listen_req2] {
            let config = listen_req.config_receiver.next().await.unwrap();
            match config {
                AccessControlOp::Remove(pk) => assert_eq!(pk, pk_c),
                _ => unreachable!(),
            };
        }
    }

    #[test]
    fn test_listen_pool_loop_update_remove_friend() {
        let thread_pool = ThreadPool::new().unwrap();
        block_on(task_listen_pool_loop_update_remove_friend(
            thread_pool.clone(),
        ));
    }
}
