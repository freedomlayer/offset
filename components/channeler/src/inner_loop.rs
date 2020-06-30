use std::cmp::Ordering;
use std::collections::HashMap;
use std::fmt::Debug;
use std::marker::Unpin;

use futures::channel::{mpsc, oneshot};
use futures::task::{Spawn, SpawnExt};
use futures::{future, select, stream, FutureExt, Sink, SinkExt, Stream, StreamExt, TryFutureExt};

use common::conn::{BoxStream, ConnPairVec, FutTransform, Listener, ListenerClient};
use common::select_streams::select_streams;
use crypto::identity::compare_public_key;

use proto::crypto::PublicKey;
use proto::funder::messages::{ChannelerToFunder, ChannelerUpdateFriend, FunderToChanneler};

use crate::connect_pool::{ConnectPoolControl, CpConfigClient, CpConnectClient};
use crate::listen_pool::LpConfig;
use crate::overwrite_channel::overwrite_send_all;

#[derive(Debug)]
pub enum ChannelerEvent<RA> {
    FromFunder(FunderToChanneler<RA>),
    Connection((PublicKey, ConnPairVec)),
    FriendEvent(FriendEvent),
    ListenerClosed,
    FunderClosed,
}

#[derive(Debug)]
pub enum FriendEvent {
    IncomingMessage((PublicKey, Vec<u8>)),
    ReceiverClosed(PublicKey),
}

#[derive(Debug)]
pub enum ChannelerError {
    SpawnError,
    ListenerError,
    SendToFunderFailed,
    AddressSendFailed,
    SendConnectionEstablishedFailed,
    SendAccessControlFailed,
    ListenerConfigError,
    ListenerClosed,
    FunderClosed,
    ConnectorConfigError,
}

struct Connected<T> {
    opt_sender: Option<mpsc::Sender<T>>,
    // TODO: Do we really need the closer here? Check it.
    #[allow(unused)]
    /// When dropped, this will trigger closing of the receiving side task:
    closer: oneshot::Sender<()>,
}

impl<T> Connected<T> {
    pub fn new(sender: mpsc::Sender<T>, closer: oneshot::Sender<()>) -> Self {
        Connected {
            opt_sender: Some(sender),
            closer,
        }
    }

    /// Send an item.
    /// If a failure occurs, the internal sender is removed
    /// and subsequent sends will fail too.
    ///
    /// Return value of true means send was successful.
    pub async fn send(&mut self, t: T) -> bool {
        match self.opt_sender.take() {
            Some(mut sender) => match sender.send(t).await {
                Ok(()) => {
                    self.opt_sender = Some(sender);
                    true
                }
                Err(_t) => false,
            },
            None => false,
        }
    }
}

type FriendConnected = Connected<Vec<u8>>;

enum InFriend {
    Listening,
    Connected(FriendConnected),
}

enum OutFriendStatus {
    Connecting,
    Connected(FriendConnected),
}

struct OutFriend<RA> {
    config_client: CpConfigClient<RA>,
    connect_client: CpConnectClient,
    status: OutFriendStatus,
}

struct Friends<RA> {
    /// Friends that should connect to us:
    in_friends: HashMap<PublicKey, InFriend>,
    /// Friends that wait for our connection:
    out_friends: HashMap<PublicKey, OutFriend<RA>>,
}

impl<RA> Friends<RA> {
    pub fn new() -> Self {
        Friends {
            in_friends: HashMap::new(),
            out_friends: HashMap::new(),
        }
    }

    /// Obtain (if possible) a FriendConnected struct corresponding to the given
    /// public_key. A FriendConnected struct allows sending messages to the remote friend.
    pub fn get_friend_connected(&mut self, public_key: &PublicKey) -> Option<&mut FriendConnected> {
        if let Some(in_friend) = self.in_friends.get_mut(public_key) {
            match in_friend {
                InFriend::Listening => {}
                InFriend::Connected(friend_connected) => return Some(friend_connected),
            }
        }

        if let Some(out_friend) = self.out_friends.get_mut(public_key) {
            match &mut out_friend.status {
                OutFriendStatus::Connecting => {}
                OutFriendStatus::Connected(friend_connected) => return Some(friend_connected),
            }
        }

        None
    }
}

struct Channeler<RA, C, S, TF> {
    local_public_key: PublicKey,
    friends: Friends<RA>,
    connector: C,
    /// Configuration sender for the listening task:
    listen_config: mpsc::Sender<LpConfig<RA>>,
    spawner: S,
    to_funder: TF,
    event_sender: mpsc::Sender<ChannelerEvent<RA>>,
}

impl<RA, C, S, TF> Channeler<RA, C, S, TF>
where
    RA: Clone + Send + Sync + 'static,
    C: FutTransform<Input = PublicKey, Output = ConnectPoolControl<RA>> + Clone + Send + 'static,
    S: Spawn + Clone + Send + 'static,
    TF: Sink<ChannelerToFunder> + Send + Unpin,
{
    fn new(
        local_public_key: PublicKey,
        connector: C,
        listen_config: mpsc::Sender<LpConfig<RA>>,
        spawner: S,
        to_funder: TF,
        event_sender: mpsc::Sender<ChannelerEvent<RA>>,
    ) -> Self {
        Channeler {
            local_public_key,
            friends: Friends::new(),
            connector,
            listen_config,
            spawner,
            to_funder,
            event_sender,
        }
    }

    /// Should we wait for a connection from `friend_public_key`.
    /// In other words: Is the remote side active?
    fn is_listen_friend(&self, friend_public_key: &PublicKey) -> bool {
        compare_public_key(&self.local_public_key, friend_public_key) == Ordering::Less
    }

    fn connect_out_friend(&mut self, friend_public_key: &PublicKey) -> Result<(), ChannelerError> {
        let out_friend = match self.friends.out_friends.get_mut(friend_public_key) {
            Some(out_friend) => out_friend,
            None => unreachable!(), // We assert that the out_friend exists.
        };

        let mut c_connect_client = out_friend.connect_client.clone();
        let c_friend_public_key = friend_public_key.clone();
        let mut c_event_sender = self.event_sender.clone();
        let connect_fut = async move {
            match c_connect_client.connect().await {
                Ok(raw_conn) => {
                    let event = ChannelerEvent::Connection((c_friend_public_key, raw_conn));
                    let _ = c_event_sender.send(event).await;
                }
                Err(e) => {
                    // This probably happened because the friend was removed
                    // during connection attempt.
                    warn!("connect_out_friend(): connect() error: {:?}", e);
                }
            };
        };

        self.spawner
            .clone()
            .spawn(connect_fut)
            .map_err(|_| ChannelerError::SpawnError)?;

        Ok(())
    }

    /// Add friend if does not yet exist
    async fn try_create_friend<'a>(
        &'a mut self,
        friend_public_key: &'a PublicKey,
    ) -> Result<(), ChannelerError> {
        if self.friends.in_friends.contains_key(friend_public_key)
            || self.friends.out_friends.contains_key(friend_public_key)
        {
            // Friend already exists:
            return Ok(());
        }

        // We should add a new friend:
        if self.is_listen_friend(friend_public_key) {
            self.friends
                .in_friends
                .insert(friend_public_key.clone(), InFriend::Listening);
        } else {
            let (config_client, connect_client) =
                self.connector.transform(friend_public_key.clone()).await;
            let out_friend = OutFriend {
                config_client,
                connect_client,
                status: OutFriendStatus::Connecting,
            };
            self.friends
                .out_friends
                .insert(friend_public_key.clone(), out_friend);
            self.connect_out_friend(friend_public_key)?;
        }
        Ok(())
    }

    async fn handle_from_funder(
        &mut self,
        funder_to_channeler: FunderToChanneler<RA>,
    ) -> Result<(), ChannelerError> {
        match funder_to_channeler {
            FunderToChanneler::Message((public_key, message)) => {
                let friend_connected = match self.friends.get_friend_connected(&public_key) {
                    Some(friend_connected) => friend_connected,
                    None => {
                        error!(
                            "Attempt to send a message to unavailable friend: {:?}",
                            public_key
                        );
                        return Ok(());
                    }
                };

                // TODO: Should we check errors here?
                let _ = friend_connected.send(message).await;
                Ok(())
            }
            FunderToChanneler::SetRelays(addresses) => {
                // Our local listening addresses were set.
                // We update the listener accordingly:
                self.listen_config
                    .send(LpConfig::SetLocalAddresses(addresses))
                    .await
                    .map_err(|_| ChannelerError::ListenerConfigError)?;

                Ok(())
            }
            FunderToChanneler::UpdateFriend(channeler_update_friend) => {
                let ChannelerUpdateFriend {
                    friend_public_key,
                    friend_relays,
                    local_relays,
                } = channeler_update_friend;

                self.try_create_friend(&friend_public_key).await?;

                if let Some(_in_friend) = self.friends.in_friends.get(&friend_public_key) {
                    let lp_config =
                        LpConfig::UpdateFriend((friend_public_key.clone(), local_relays));
                    self.listen_config
                        .send(lp_config)
                        .await
                        .map_err(|_| ChannelerError::ListenerConfigError)?;
                } else if let Some(out_friend) =
                    self.friends.out_friends.get_mut(&friend_public_key)
                {
                    out_friend
                        .config_client
                        .config(friend_relays)
                        .await
                        .map_err(|_| ChannelerError::ConnectorConfigError)?;
                }

                Ok(())
            }
            FunderToChanneler::RemoveFriend(friend_public_key) => {
                if self.friends.in_friends.remove(&friend_public_key).is_some() {
                    let lp_config = LpConfig::RemoveFriend(friend_public_key.clone());
                    self.listen_config
                        .send(lp_config)
                        .await
                        .map_err(|_| ChannelerError::ListenerConfigError)?;
                    return Ok(());
                }

                self.friends.out_friends.remove(&friend_public_key);

                Ok(())
            }
        }
    }

    /// Handle incoming connection from a remote friend
    async fn handle_connection(
        &mut self,
        friend_public_key: PublicKey,
        conn_pair: ConnPairVec,
    ) -> Result<(), ChannelerError> {
        let (sender, receiver) = conn_pair.split();

        // Close fut_recv whenever closer is closed.
        let (closer, close_receiver) = oneshot::channel::<()>();

        // We use an overwrite channel to make sure we are never stuck on trying to send a
        // message to remote friend. A friend only needs to know the most recent message,
        // so previous pending messages may be discarded.
        let (friend_sender, friend_receiver) = mpsc::channel(0);
        self.spawner
            .spawn(
                overwrite_send_all(sender, friend_receiver)
                    .map_err(|e| error!("overwrite_send_all() error: {:?}", e))
                    .map(|_| ()),
            )
            .map_err(|_| ChannelerError::SpawnError)?;

        if let Some(in_friend) = self.friends.in_friends.get_mut(&friend_public_key) {
            match in_friend {
                InFriend::Connected(_) => {
                    warn!(
                        "Already connected to in_friend: {:?}. Aborting.",
                        friend_public_key
                    );
                    return Ok(());
                }
                InFriend::Listening => {
                    *in_friend = InFriend::Connected(Connected::new(friend_sender, closer))
                }
            }
        } else if let Some(mut out_friend) = self.friends.out_friends.get_mut(&friend_public_key) {
            match out_friend.status {
                OutFriendStatus::Connected(_) => {
                    warn!(
                        "Already connected to out_friend: {:?}. Aborting.",
                        friend_public_key
                    );
                    return Ok(());
                }
                OutFriendStatus::Connecting => {
                    out_friend.status =
                        OutFriendStatus::Connected(Connected::new(friend_sender, closer))
                }
            }
        } else {
            //  This might happen if an out_friend was added and then suddenly removed.
            //  We might get the connection success event but we don't want to connect anymore.
            warn!("handle_connection(): Not an in_friend or an out_friend. Aborting");
            return Ok(());
        }

        let mut c_event_sender = self.event_sender.clone();
        let c_friend_public_key = friend_public_key.clone();
        let mut receiver = receiver
            .map(move |data| {
                ChannelerEvent::FriendEvent(FriendEvent::IncomingMessage((
                    c_friend_public_key.clone(),
                    data,
                )))
            })
            .map(Ok);
        let c_friend_public_key = friend_public_key.clone();
        let fut_recv = async move {
            select! {
                _ = c_event_sender.send_all(&mut receiver).fuse() => (),
                _ = close_receiver.fuse() => (),
            };

            let receiver_closed_event = ChannelerEvent::FriendEvent(FriendEvent::ReceiverClosed(
                c_friend_public_key.clone(),
            ));
            let _ = c_event_sender.send(receiver_closed_event).await;
        };

        self.spawner
            .spawn(fut_recv)
            .map_err(|_| ChannelerError::SpawnError)?;

        // Report to Funder that the friend is online:
        let to_funder = ChannelerToFunder::Online(friend_public_key.clone());
        self.to_funder
            .send(to_funder)
            .await
            .map_err(|_| ChannelerError::SendToFunderFailed)?;

        Ok(())
    }

    async fn handle_friend_event(
        &mut self,
        friend_event: FriendEvent,
    ) -> Result<(), ChannelerError> {
        match friend_event {
            FriendEvent::IncomingMessage((friend_public_key, data)) => {
                let message = ChannelerToFunder::Message((friend_public_key, data));
                self.to_funder
                    .send(message)
                    .await
                    .map_err(|_| ChannelerError::SendToFunderFailed)?
            }
            FriendEvent::ReceiverClosed(friend_public_key) => {
                // Report Funder that the friend is offline:
                let to_funder = ChannelerToFunder::Offline(friend_public_key.clone());
                self.to_funder
                    .send(to_funder)
                    .await
                    .map_err(|_| ChannelerError::SendToFunderFailed)?;

                /*
                if self
                    .friends
                    .get_friend_connected(&friend_public_key)
                    .is_some()
                {
                    // Report Funder that the friend is offline:
                    let to_funder = ChannelerToFunder::Offline(friend_public_key.clone());
                    self.to_funder.send(to_funder).await
                        .map_err(|_| ChannelerError::SendToFunderFailed)?;
                }
                */

                if let Some(in_friend) = self.friends.in_friends.get_mut(&friend_public_key) {
                    *in_friend = InFriend::Listening;
                } else if let Some(out_friend) =
                    self.friends.out_friends.get_mut(&friend_public_key)
                {
                    // Request a new connection
                    out_friend.status = OutFriendStatus::Connecting;
                    self.connect_out_friend(&friend_public_key)?;
                }
            }
        }
        Ok(())
    }
}

pub async fn channeler_loop_inner<FF, TF, RA, C, L, S>(
    local_public_key: PublicKey,
    from_funder: FF,
    to_funder: TF,
    connector: C,
    listener: L,
    spawner: S,
) -> Result<(), ChannelerError>
where
    FF: Stream<Item = FunderToChanneler<RA>> + Send + Unpin,
    TF: Sink<ChannelerToFunder> + Send + Unpin,
    RA: Clone + Send + Sync + Debug + 'static,
    C: FutTransform<Input = PublicKey, Output = ConnectPoolControl<RA>> + Clone + Send + 'static,
    L: Listener<Connection = (PublicKey, ConnPairVec), Config = LpConfig<RA>, Arg = ()>
        + Clone
        + Send,
    S: Spawn + Clone + Send + 'static,
{
    let (event_sender, event_receiver) = mpsc::channel(0);

    // Pool Listener should never fail:
    let ListenerClient {
        config_sender: listen_config,
        conn_receiver: incoming_listen_conns,
    } = listener
        .listen(())
        .await
        .map_err(|_| ChannelerError::ListenerError)?;

    let mut channeler = Channeler::new(
        local_public_key,
        connector,
        listen_config,
        spawner,
        to_funder,
        event_sender,
    );

    // Forward incoming listen connections:
    let mut c_event_sender = channeler.event_sender.clone();
    let incoming_listen_conns = incoming_listen_conns.map(ChannelerEvent::Connection);
    let send_listen_conns_fut = async move {
        let _ = c_event_sender
            .send_all(&mut incoming_listen_conns.map(Ok))
            .await;
        // If we reach here it means an error occurred.
        let _ = c_event_sender.send(ChannelerEvent::ListenerClosed).await;
    };
    channeler
        .spawner
        .spawn(send_listen_conns_fut)
        .map_err(|_| ChannelerError::SpawnError)?;

    let from_funder = from_funder
        .map(ChannelerEvent::FromFunder)
        .chain(stream::once(future::ready(ChannelerEvent::FunderClosed)));

    let mut events = select_streams![event_receiver, from_funder];

    while let Some(event) = events.next().await {
        match event {
            ChannelerEvent::FromFunder(funder_to_channeler) => {
                channeler.handle_from_funder(funder_to_channeler).await?
            }
            ChannelerEvent::Connection((public_key, raw_conn)) => {
                channeler.handle_connection(public_key, raw_conn).await?
            }
            ChannelerEvent::FriendEvent(friend_event) => {
                channeler.handle_friend_event(friend_event).await?
            }
            ChannelerEvent::ListenerClosed => return Err(ChannelerError::ListenerClosed),
            ChannelerEvent::FunderClosed => return Err(ChannelerError::FunderClosed),
        };
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::executor::{block_on, ThreadPool};

    use common::dummy_connector::DummyConnector;
    use common::dummy_listener::DummyListener;
    use proto::crypto::PublicKey;

    /// Test the case of a friend the channeler initiates connection to.
    async fn task_channeler_loop_connect_friend<S>(spawner: S)
    where
        S: Spawn + Clone + Send + 'static,
    {
        let (mut funder_sender, from_funder) = mpsc::channel(0);
        let (to_funder, mut funder_receiver) = mpsc::channel(0);

        // We sort the public keys ahead of time, so that we know how to break ties.
        // Our local public key will be pks[1]. pks[0] < pks[1] < pks[2]
        //
        // pks[1] >= pks[0], so pks[0] be an active send friend (We initiate connection)
        // pks[1] < pks[2], hence pks[2] will be a listen friend. (We wait for him to connect)
        let mut pks = (0..3)
            .map(|i| PublicKey::from(&[i; PublicKey::len()]))
            .collect::<Vec<PublicKey>>();
        pks.sort_by(compare_public_key);

        let (conn_request_sender, mut conn_request_receiver) = mpsc::channel(0);
        let connector = DummyConnector::new(conn_request_sender);

        let (listener_req_sender, mut listener_req_receiver) = mpsc::channel(0);
        let listener = DummyListener::new(listener_req_sender);

        spawner
            .spawn(
                channeler_loop_inner(
                    pks[1].clone(),
                    from_funder,
                    to_funder,
                    connector,
                    listener,
                    spawner.clone(),
                )
                .map_err(|e| error!("Error in channeler_loop(): {:?}", e))
                .map(|_| ()),
            )
            .unwrap();

        let mut listener_request = listener_req_receiver.next().await.unwrap();

        // Play with changing relay addresses:
        funder_sender
            .send(FunderToChanneler::SetRelays(vec![0x1337u32]))
            .await
            .unwrap();

        let lp_config = listener_request.config_receiver.next().await.unwrap();
        match lp_config {
            LpConfig::SetLocalAddresses(addresses) => assert_eq!(addresses, vec![0x1337u32]),
            _ => unreachable!(),
        };

        // Empty relay address:
        funder_sender
            .send(FunderToChanneler::SetRelays(vec![]))
            .await
            .unwrap();

        let lp_config = listener_request.config_receiver.next().await.unwrap();
        match lp_config {
            LpConfig::SetLocalAddresses(addresses) => assert_eq!(addresses, Vec::<u32>::new()),
            _ => unreachable!(),
        };

        // This is the final address we set for our relay:
        funder_sender
            .send(FunderToChanneler::SetRelays(vec![0x1u32]))
            .await
            .unwrap();

        let lp_config = listener_request.config_receiver.next().await.unwrap();
        match lp_config {
            LpConfig::SetLocalAddresses(addresses) => assert_eq!(addresses, vec![0x1u32]),
            _ => unreachable!(),
        };

        // Add a friend:
        let channeler_update_friend = ChannelerUpdateFriend {
            friend_public_key: pks[0].clone(),
            friend_relays: vec![0x0u32],
            local_relays: vec![0x2u32, 0x3u32],
        };

        funder_sender
            .send(FunderToChanneler::UpdateFriend(channeler_update_friend))
            .await
            .unwrap();
        let conn_request = conn_request_receiver.next().await.unwrap();
        assert_eq!(conn_request.address, pks[0]);
        let (connect_sender0, mut connect_receiver0) = mpsc::channel(0);
        let (config_sender0, mut config_receiver0) = mpsc::channel(0);

        let config_client0 = CpConfigClient::new(config_sender0);
        let connect_client0 = CpConnectClient::new(connect_sender0);
        conn_request.reply((config_client0, connect_client0));

        let config0 = config_receiver0.next().await.unwrap();
        assert_eq!(config0, vec![0x0u32]);

        let connect_req0 = connect_receiver0.next().await.unwrap();

        // Send back a connection:
        let (mut pk0_sender, local_receiver) = mpsc::channel(0);
        let (local_sender, mut pk0_receiver) = mpsc::channel(0);
        connect_req0
            .response_sender
            .send(ConnPairVec::from_raw(local_sender, local_receiver))
            .unwrap();

        // Friend should be reported as online:
        let channeler_to_funder = funder_receiver.next().await.unwrap();
        match channeler_to_funder {
            ChannelerToFunder::Online(public_key) => assert_eq!(public_key, pks[0]),
            _ => unreachable!(),
        };

        // Send a message to pks[0]:
        funder_sender
            .send(FunderToChanneler::Message((pks[0].clone(), vec![1, 2, 3])))
            .await
            .unwrap();
        assert_eq!(pk0_receiver.next().await.unwrap(), vec![1, 2, 3]);

        // Send a message from pks[0]:
        pk0_sender.send(vec![3, 2, 1]).await.unwrap();

        // We expect to get the message from pks[0]:
        let channeler_to_funder = funder_receiver.next().await.unwrap();
        match channeler_to_funder {
            ChannelerToFunder::Message((public_key, message)) => {
                assert_eq!(public_key, pks[0]);
                assert_eq!(message, vec![3, 2, 1]);
            }
            _ => unreachable!(),
        };

        // Drop pks[0] connection:
        drop(pk0_sender);
        drop(pk0_receiver);

        // pks[0] should be reported as offline:
        let channeler_to_funder = funder_receiver.next().await.unwrap();
        match channeler_to_funder {
            ChannelerToFunder::Offline(public_key) => assert_eq!(public_key, pks[0]),
            _ => unreachable!(),
        };

        // Connection to pks[0] should be attempted again:
        let connect_req0 = connect_receiver0.next().await.unwrap();

        let (pk0_sender, local_receiver) = mpsc::channel(0);
        let (local_sender, pk0_receiver) = mpsc::channel(0);
        connect_req0
            .response_sender
            .send(ConnPairVec::from_raw(local_sender, local_receiver))
            .unwrap();

        // Online report:
        let channeler_to_funder = funder_receiver.next().await.unwrap();
        match channeler_to_funder {
            ChannelerToFunder::Online(public_key) => assert_eq!(public_key, pks[0]),
            _ => unreachable!(),
        };

        // Drop pks[0] connection:
        drop(pk0_sender);
        drop(pk0_receiver);

        // Offline report:
        let channeler_to_funder = funder_receiver.next().await.unwrap();
        match channeler_to_funder {
            ChannelerToFunder::Offline(public_key) => assert_eq!(public_key, pks[0]),
            _ => unreachable!(),
        };

        // A new connection is attempted:
        let connect_req0 = connect_receiver0.next().await.unwrap();

        // Remove friend:
        funder_sender
            .send(FunderToChanneler::RemoveFriend(pks[0].clone()))
            .await
            .unwrap();

        let (_pk0_sender, local_receiver) = mpsc::channel(0);
        let (local_sender, _pk0_receiver) = mpsc::channel(0);
        drop(
            connect_req0
                .response_sender
                .send(ConnPairVec::from_raw(local_sender, local_receiver)),
        );

        // The connection requests receiver should be closed:
        assert!(connect_receiver0.next().await.is_none());
    }

    #[test]
    fn test_channeler_loop_connect_friend() {
        let thread_pool = ThreadPool::new().unwrap();
        block_on(task_channeler_loop_connect_friend(thread_pool.clone()));
    }

    // ------------------------------------------------------------
    // ------------------------------------------------------------

    /// Test the case of the channeler waiting for a connection from a friend.
    async fn task_channeler_loop_listen_friend<S>(spawner: S)
    where
        S: Spawn + Clone + Send + 'static,
    {
        let (mut funder_sender, from_funder) = mpsc::channel(0);
        let (to_funder, mut funder_receiver) = mpsc::channel(0);

        // We sort the public keys ahead of time, so that we know how to break ties.
        // Our local public key will be pks[1]. pks[0] < pks[1] < pks[2]
        //
        // pks[1] >= pks[0], so pks[0] be an active send friend (We initiate connection)
        // pks[1] < pks[2], hence pks[2] will be a listen friend. (We wait for him to connect)
        let mut pks = (0..3)
            .map(|i| PublicKey::from(&[i; PublicKey::len()]))
            .collect::<Vec<PublicKey>>();
        pks.sort_by(compare_public_key);

        let (conn_request_sender, _conn_request_receiver) = mpsc::channel(0);
        let connector = DummyConnector::new(conn_request_sender);

        let (listener_req_sender, mut listener_req_receiver) = mpsc::channel(0);
        let listener = DummyListener::new(listener_req_sender);

        spawner
            .spawn(
                channeler_loop_inner(
                    pks[1].clone(),
                    from_funder,
                    to_funder,
                    connector,
                    listener,
                    spawner.clone(),
                )
                .map_err(|e| error!("Error in channeler_loop(): {:?}", e))
                .map(|_| ()),
            )
            .unwrap();

        // Set address for our relay:
        funder_sender
            .send(FunderToChanneler::SetRelays(vec![0x1u32]))
            .await
            .unwrap();
        let mut listener_request = listener_req_receiver.next().await.unwrap();

        let lp_config = listener_request.config_receiver.next().await.unwrap();
        assert_eq!(lp_config, LpConfig::SetLocalAddresses(vec![0x1u32]));

        // Add a friend:
        let channeler_update_friend = ChannelerUpdateFriend {
            friend_public_key: pks[2].clone(),
            friend_relays: vec![0x0u32],
            local_relays: vec![0x2u32, 0x3u32],
        };
        funder_sender
            .send(FunderToChanneler::UpdateFriend(channeler_update_friend))
            .await
            .unwrap();

        let lp_config = listener_request.config_receiver.next().await.unwrap();
        assert_eq!(
            lp_config,
            LpConfig::UpdateFriend((pks[2].clone(), vec![0x2u32, 0x3u32]))
        );

        // Set up connection, exchange messages and close the connection a few times:
        for _ in 0..3 {
            // The channeler now listens. It waits for an incoming connection from pks[2]
            // Set up a connection from pks[2]:
            let (mut pk2_sender, receiver) = mpsc::channel(0);
            let (sender, mut pk2_receiver) = mpsc::channel(0);
            listener_request
                .conn_sender
                .send((pks[2].clone(), ConnPairVec::from_raw(sender, receiver)))
                .await
                .unwrap();

            // Friend should be reported as online:
            let channeler_to_funder = funder_receiver.next().await.unwrap();
            match channeler_to_funder {
                ChannelerToFunder::Online(public_key) => assert_eq!(public_key, pks[2]),
                _ => unreachable!(),
            };

            // Send a message to pks[2]:
            funder_sender
                .send(FunderToChanneler::Message((pks[2].clone(), vec![1, 2, 3])))
                .await
                .unwrap();
            assert_eq!(pk2_receiver.next().await.unwrap(), vec![1, 2, 3]);

            // Send a message from pks2:
            pk2_sender.send(vec![3, 2, 1]).await.unwrap();

            // We expect to get the message from pks[2]:
            let channeler_to_funder = funder_receiver.next().await.unwrap();
            match channeler_to_funder {
                ChannelerToFunder::Message((public_key, message)) => {
                    assert_eq!(public_key, pks[2]);
                    assert_eq!(message, vec![3, 2, 1]);
                }
                _ => unreachable!(),
            };

            // Drop pks[2] connection:
            drop(pk2_sender);
            drop(pk2_receiver);

            // Friend should be reported as offline:
            let channeler_to_funder = funder_receiver.next().await.unwrap();
            match channeler_to_funder {
                ChannelerToFunder::Offline(public_key) => assert_eq!(public_key, pks[2]),
                _ => unreachable!(),
            };
        }

        // Remove friend:
        funder_sender
            .send(FunderToChanneler::RemoveFriend(pks[2].clone()))
            .await
            .unwrap();
    }

    #[test]
    fn test_channeler_loop_listen_friend() {
        let thread_pool = ThreadPool::new().unwrap();
        block_on(task_channeler_loop_listen_friend(thread_pool.clone()));
    }

    // ------------------------------------------------------------
    // ------------------------------------------------------------

    /// Test multiple updating and removing the same friend
    /// The friend is removed in the middle of a connection. We expect the connection to be
    /// forcefully closed.
    async fn task_channeler_loop_update_remove_friend<S>(spawner: S)
    where
        S: Spawn + Clone + Send + 'static,
    {
        let (mut funder_sender, from_funder) = mpsc::channel(0);
        let (to_funder, mut funder_receiver) = mpsc::channel(0);

        // We sort the public keys ahead of time, so that we know how to break ties.
        // Our local public key will be pks[1]. pks[0] < pks[1] < pks[2]
        //
        // pks[1] >= pks[0], so pks[0] be an active send friend (We initiate connection)
        // pks[1] < pks[2], hence pks[2] will be a listen friend. (We wait for him to connect)
        let mut pks = (0..3)
            .map(|i| PublicKey::from(&[i; PublicKey::len()]))
            .collect::<Vec<PublicKey>>();
        pks.sort_by(compare_public_key);

        let (conn_request_sender, _conn_request_receiver) = mpsc::channel(0);
        let connector = DummyConnector::new(conn_request_sender);

        let (listener_req_sender, mut listener_req_receiver) = mpsc::channel(0);
        let listener = DummyListener::new(listener_req_sender);

        spawner
            .spawn(
                channeler_loop_inner(
                    pks[1].clone(),
                    from_funder,
                    to_funder,
                    connector,
                    listener,
                    spawner.clone(),
                )
                .map_err(|e| error!("Error in channeler_loop(): {:?}", e))
                .map(|_| ()),
            )
            .unwrap();

        // Set address for our relay:
        funder_sender
            .send(FunderToChanneler::SetRelays(vec![0x1u32]))
            .await
            .unwrap();
        let mut listener_request = listener_req_receiver.next().await.unwrap();

        let lp_config = listener_request.config_receiver.next().await.unwrap();
        assert_eq!(lp_config, LpConfig::SetLocalAddresses(vec![0x1u32]));

        for _ in 0..3 {
            // Add a friend:
            let channeler_update_friend = ChannelerUpdateFriend {
                friend_public_key: pks[2].clone(),
                friend_relays: vec![0x0u32],
                local_relays: vec![0x2u32, 0x3u32],
            };
            funder_sender
                .send(FunderToChanneler::UpdateFriend(channeler_update_friend))
                .await
                .unwrap();

            let lp_config = listener_request.config_receiver.next().await.unwrap();
            assert_eq!(
                lp_config,
                LpConfig::UpdateFriend((pks[2].clone(), vec![0x2u32, 0x3u32]))
            );

            // Set up a connection:
            let (_pk2_sender, receiver) = mpsc::channel(0);
            let (sender, _pk2_receiver) = mpsc::channel(0);
            listener_request
                .conn_sender
                .send((pks[2].clone(), ConnPairVec::from_raw(sender, receiver)))
                .await
                .unwrap();

            // Friend should be reported as online:
            let channeler_to_funder = funder_receiver.next().await.unwrap();
            match channeler_to_funder {
                ChannelerToFunder::Online(public_key) => assert_eq!(public_key, pks[2]),
                _ => unreachable!(),
            };

            // Request to remove the friend in the middle of connection:
            funder_sender
                .send(FunderToChanneler::RemoveFriend(pks[2].clone()))
                .await
                .unwrap();

            let lp_config = listener_request.config_receiver.next().await.unwrap();
            assert_eq!(lp_config, LpConfig::RemoveFriend(pks[2].clone()));

            // Friend should be reported as offline:
            let channeler_to_funder = funder_receiver.next().await.unwrap();
            match channeler_to_funder {
                ChannelerToFunder::Offline(public_key) => assert_eq!(public_key, pks[2]),
                _ => unreachable!(),
            };
        }
    }

    #[test]
    fn test_channeler_loop_update_remove_friend() {
        let thread_pool = ThreadPool::new().unwrap();
        block_on(task_channeler_loop_update_remove_friend(
            thread_pool.clone(),
        ));
    }

    /// Test the case of a friend the channeler initiates connection to, and suddenly the friend is
    /// removed
    async fn task_channeler_loop_connect_friend_removed<S>(spawner: S)
    where
        S: Spawn + Clone + Send + 'static,
    {
        let (mut funder_sender, from_funder) = mpsc::channel(1);
        let (to_funder, _funder_receiver) = mpsc::channel(1);

        // We sort the public keys ahead of time, so that we know how to break ties.
        // Our local public key will be pks[1]. pks[0] < pks[1] < pks[2]
        //
        // pks[1] >= pks[0], so pks[0] be an active send friend (We initiate connection)
        // pks[1] < pks[2], hence pks[2] will be a listen friend. (We wait for him to connect)
        let mut pks = (0..3)
            .map(|i| PublicKey::from(&[i; PublicKey::len()]))
            .collect::<Vec<PublicKey>>();
        pks.sort_by(compare_public_key);

        let (conn_request_sender, mut conn_request_receiver) = mpsc::channel(1);
        let connector = DummyConnector::new(conn_request_sender);

        let (listener_req_sender, mut listener_req_receiver) = mpsc::channel(1);
        let listener = DummyListener::new(listener_req_sender);

        spawner
            .spawn(
                channeler_loop_inner(
                    pks[1].clone(),
                    from_funder,
                    to_funder,
                    connector,
                    listener,
                    spawner.clone(),
                )
                .map_err(|e| error!("Error in channeler_loop(): {:?}", e))
                .map(|_| ()),
            )
            .unwrap();

        let mut listener_request = listener_req_receiver.next().await.unwrap();

        // This is the final address we set for our relay:
        funder_sender
            .send(FunderToChanneler::SetRelays(vec![0x1u32]))
            .await
            .unwrap();

        let lp_config = listener_request.config_receiver.next().await.unwrap();
        match lp_config {
            LpConfig::SetLocalAddresses(addresses) => assert_eq!(addresses, vec![0x1u32]),
            _ => unreachable!(),
        };

        // Add a friend:
        let channeler_update_friend = ChannelerUpdateFriend {
            friend_public_key: pks[0].clone(),
            friend_relays: vec![0x0u32],
            local_relays: vec![0x2u32, 0x3u32],
        };

        funder_sender
            .send(FunderToChanneler::UpdateFriend(
                channeler_update_friend.clone(),
            ))
            .await
            .unwrap();
        let conn_request = conn_request_receiver.next().await.unwrap();
        assert_eq!(conn_request.address, pks[0]);

        // Request to remove the friend in the middle of connection attempt:
        funder_sender
            .send(FunderToChanneler::RemoveFriend(pks[0].clone()))
            .await
            .unwrap();

        // Reply to the conn request too late:
        let (connect_sender0, _connect_receiver0) = mpsc::channel(1);
        let (config_sender0, _config_receiver0) = mpsc::channel(1);

        let config_client0 = CpConfigClient::new(config_sender0);
        let connect_client0 = CpConnectClient::new(connect_sender0);
        conn_request.reply((config_client0, connect_client0));

        // UpdateFriend again, to make sure channeler is still alive:
        funder_sender
            .send(FunderToChanneler::UpdateFriend(channeler_update_friend))
            .await
            .unwrap();

        let conn_request = conn_request_receiver.next().await.unwrap();
        assert_eq!(conn_request.address, pks[0]);

        // Reply to the conn request, to avoid panic on exit:
        let (connect_sender0, _connect_receiver0) = mpsc::channel(1);
        let (config_sender0, _config_receiver0) = mpsc::channel(1);

        let config_client0 = CpConfigClient::new(config_sender0);
        let connect_client0 = CpConnectClient::new(connect_sender0);
        conn_request.reply((config_client0, connect_client0));
    }

    #[test]
    fn test_channeler_loop_connect_friend_removed() {
        let thread_pool = ThreadPool::new().unwrap();
        block_on(task_channeler_loop_connect_friend_removed(
            thread_pool.clone(),
        ));
    }

    // TODO: Add tests to make sure access control works properly?
    // If a friend with a strange public key tries to connect, he should not be able to succeed?
}
