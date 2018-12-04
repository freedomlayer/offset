use std::marker::Unpin;
use std::collections::HashMap;

use futures::{future, FutureExt, TryFutureExt, stream, Stream, StreamExt, Sink, SinkExt};
use futures::task::{Spawn, SpawnExt};
use futures::channel::{oneshot, mpsc};

use proto::funder::messages::{FunderToChanneler, ChannelerToFunder};

use crypto::identity::PublicKey;
use crypto::crypto_rand::CryptoRandom;

use timer::TimerClient;

use identity::IdentityClient;

use relay::client::connector::{Connector, ConnPair};
use relay::client::access_control::{AccessControl, AccessControlOp};

use crate::listen::listen_loop;
use crate::connect::{connect, ConnectError};
use crate::overwrite_channel::overwrite_send_all;

pub enum ChannelerEvent<A> {
    FromFunder(FunderToChanneler<A>),
    Connection((PublicKey, ConnPair<Vec<u8>, Vec<u8>>)),
    FriendEvent(FriendEvent),
}

pub enum FriendEvent {
    IncomingMessage((PublicKey, Vec<u8>)),
    ReceiverClosed(PublicKey),
}

#[derive(Debug)]
pub enum ChannelerError {
    SpawnError,
    SendToFunderFailed,
    AddressSendFailed,
    SendConnectionEstablishedFailed,
    SendAccessControlFailed,
}

struct Friend<A> {
    opt_address: Option<A>,
    state: FriendState,
}

struct FriendConnected {
    opt_sender: Option<mpsc::Sender<Vec<u8>>>,
}

struct FriendInitiating {
    close_sender: oneshot::Sender<()>,
}

enum FriendState {
    Connected(FriendConnected),
    Initiating(FriendInitiating),
    Listening,
}

struct Listening {
    close_sender: oneshot::Sender<()>,
    access_control_sender: mpsc::Sender<AccessControlOp>,
}

enum ListenState {
    Listening(Listening), 
    Idle,
}

struct Channeler<A,C,TF,R,S> {
    opt_address: Option<A>,
    friends: HashMap<PublicKey, Friend<A>>,
    listen_state: ListenState,
    connector: C,
    to_funder: TF,
    timer_client: TimerClient,
    conn_timeout_ticks: usize,
    keepalive_ticks: usize,
    backoff_ticks: usize,
    identity_client: IdentityClient,
    rng: R,
    spawner: S,
    friend_event_sender: mpsc::Sender<FriendEvent>,
    connections_sender: mpsc::Sender<(PublicKey, ConnPair<Vec<u8>, Vec<u8>>)>,
}

impl<A,C,TF,R,S> Channeler<A,C,TF,R,S> 
where
    A: Clone + Send + Sync + 'static,
    C: Connector<Address=A, SendItem=Vec<u8>, RecvItem=Vec<u8>> + Clone + Send + Sync + 'static,
    TF: Sink<SinkItem=ChannelerToFunder> + Unpin,
    R: CryptoRandom,
    S: Spawn + Clone + Send + Sync + 'static,
{
    fn new(connector: C, 
           to_funder: TF,
           timer_client: TimerClient,
           conn_timeout_ticks: usize,
           keepalive_ticks: usize,
           backoff_ticks: usize,
           identity_client: IdentityClient,
           rng: R,
           spawner: S,
           friend_event_sender: mpsc::Sender<FriendEvent>,
           connections_sender: mpsc::Sender<(PublicKey, ConnPair<Vec<u8>, Vec<u8>>)>) -> Channeler<A,C,TF,R,S> {
        

        Channeler { 
            opt_address: None,
            friends: HashMap::new(),
            listen_state: ListenState::Idle,
            connector,
            to_funder,
            timer_client,
            conn_timeout_ticks,
            keepalive_ticks,
            backoff_ticks,
            identity_client,
            rng,
            spawner,
            friend_event_sender,
            connections_sender,
        }
    }

    /// Create AccessControl according to current configured friends.
    /// Only listening friends are included.
    fn create_access_control(&self) -> AccessControl {
        let mut access_control = AccessControl::new();
        for (public_key, friend) in &self.friends {
            if friend.opt_address.is_none() {
                access_control.apply_op(AccessControlOp::Add(public_key.clone()));
            }
        }
        access_control
    }
}

fn spawn_friend<A,C,TF,R,S>(channeler: &mut Channeler<A,C,TF,R,S>, 
                              public_key: PublicKey, opt_address: Option<A>) 
    -> Result<Friend<A>, ChannelerError>
where
    A: Clone + Send + Sync + 'static,
    C: Connector<Address=A, SendItem=Vec<u8>, RecvItem=Vec<u8>> + Clone + Send + Sync + 'static,
    TF: Sink<SinkItem=ChannelerToFunder> + Unpin,
    R: CryptoRandom + 'static,
    S: Spawn + Clone + Send + Sync + 'static,
{
    let friend_state = match opt_address.clone() {
        Some(address) => {
            let (close_sender, close_receiver) = oneshot::channel::<()>();

            let c_connector = channeler.connector.clone();
            let c_keepalive_ticks = channeler.keepalive_ticks;
            let c_backoff_ticks = channeler.backoff_ticks;
            let c_timer_client = channeler.timer_client.clone();
            let c_identity_client = channeler.identity_client.clone();
            let c_rng = channeler.rng.clone();
            let c_spawner = channeler.spawner.clone();
            let mut c_connections_sender = channeler.connections_sender.clone();

            let c_public_key = public_key.clone();

            let connect_fut = async move {
                let res = await!(connect(c_connector.clone(),
                    address,
                    c_public_key.clone(),
                    c_keepalive_ticks,
                    c_backoff_ticks,
                    c_timer_client,
                    close_receiver,
                    c_identity_client,
                    c_rng,
                    c_spawner));

                match res {
                    Ok(conn_pair) => {
                        await!(c_connections_sender.send((c_public_key.clone(), conn_pair)))
                            .map_err(|_| ChannelerError::SendConnectionEstablishedFailed)?;
                    },
                    Err(ConnectError::Canceled) => {},
                };
                Ok(())
            }.map_err(|e: ChannelerError| panic!("Error in connect_fut: {:?}", e))
            .map(|_| ());

            channeler.spawner.spawn(connect_fut)
                .map_err(|_| ChannelerError::SpawnError)?;

            FriendState::Initiating(FriendInitiating {
                close_sender,
            })
        },
        None => {
            FriendState::Listening
        },
    };
    let friend = Friend {
        opt_address,
        state: friend_state,
    };
    Ok(friend)
}

async fn handle_from_funder<A,C,TF,R,S>(channeler: &mut Channeler<A,C,TF,R,S>, 
                         funder_to_channeler: FunderToChanneler<A>) 
    -> Result<(), ChannelerError>  
where
    A: Clone + Send + Sync + 'static,
    C: Connector<Address=A, SendItem=Vec<u8>, RecvItem=Vec<u8>> + Clone + Send + Sync + 'static,
    TF: Sink<SinkItem=ChannelerToFunder> + Unpin,
    R: CryptoRandom + 'static,
    S: Spawn + Clone + Send + Sync + 'static,
{

    match funder_to_channeler {
        FunderToChanneler::Message((public_key, message)) => {
            let friend = match channeler.friends.get_mut(&public_key) {
                Some(friend) => friend,
                None => {
                    error!("Attempt to send a message to unavailable friend: {:?}", public_key);
                    return Ok(());
                },
            };

            let friend_connected = match &mut friend.state {
                FriendState::Listening |
                FriendState::Initiating(_) => {
                    error!("Attempt to send a message to unconnected friend: {:?}", public_key);
                    return Ok(());
                },
                FriendState::Connected(friend_connected) => friend_connected,
            };

            let mut sender = match friend_connected.opt_sender.take() {
                None => {
                    error!("No sender exists for friend: {:?}", public_key);
                    return Ok(());
                }
                Some(sender) => sender,
            };

            if let Ok(()) = await!(sender.send(message)) {
                    friend_connected.opt_sender = Some(sender);
            } else {
                error!("Error sending message to friend: {:?}", public_key);
            }

            Ok(())
        },
        FunderToChanneler::SetAddress(opt_address) => {
            match opt_address {
                Some(address) => {
                    let (close_sender, close_receiver) = oneshot::channel();
                    let (access_control_sender, access_control_receiver) = mpsc::channel(0);

                    let listen_loop_fut = listen_loop(channeler.connector.clone(),
                                      address,
                                      access_control_receiver,
                                      channeler.connections_sender.clone(),
                                      close_receiver,
                                      channeler.create_access_control(),
                                      channeler.conn_timeout_ticks,
                                      channeler.keepalive_ticks,
                                      channeler.backoff_ticks,
                                      channeler.timer_client.clone(),
                                      channeler.identity_client.clone(),
                                      channeler.rng.clone(),
                                      channeler.spawner.clone())
                        .map_err(|e| {
                            panic!("[Channeler] listen_loop() error: {:?}", e);
                        }).map(|_| ());

                    channeler.spawner.spawn(listen_loop_fut)
                        .map_err(|_| ChannelerError::SpawnError)?;

                    let listening = Listening {
                        close_sender,
                        access_control_sender,
                    };

                    channeler.listen_state = ListenState::Listening(listening);
                },
                None => {
                    channeler.listen_state = ListenState::Idle;
                    // Note: This should also destory the current listening task (If there is any), 
                    // because the close_sender is being dropped.
                },
            }
            Ok(())
        },
        FunderToChanneler::AddFriend((public_key, opt_address)) => {
            if channeler.friends.contains_key(&public_key) {
                error!("Friend {:?} already exists! Aborting.", public_key);
                return Ok(());
            }
            let friend = spawn_friend(channeler, public_key.clone(), opt_address.clone())?;
            channeler.friends.insert(public_key.clone(), friend);

            if let Some(_) = opt_address {
                if let ListenState::Listening(ref mut listening) = channeler.listen_state {
                    await!(listening.access_control_sender.send(AccessControlOp::Add(public_key.clone())))
                                    .map_err(|_| ChannelerError::SendAccessControlFailed)?;
                }
            }
            Ok(())
        },
        FunderToChanneler::RemoveFriend(public_key) => {
            let opt_friend = channeler.friends.remove(&public_key);
            match opt_friend {
                None => error!("Friend {:?} does not exist! Aborting.", public_key),
                Some(friend) => {
                    if let None = friend.opt_address {
                        if let ListenState::Listening(listening) = &mut channeler.listen_state {
                            await!(listening.access_control_sender.send(AccessControlOp::Remove(public_key)))
                                            .map_err(|_| ChannelerError::SendAccessControlFailed)?;
                        }
                    }
                }
            }
            Ok(())
        },
    }
}

#[allow(unused)]
async fn channeler_loop<FF,TF,C,A,R,S>(
                        from_funder: FF, 
                        to_funder: TF,
                        conn_timeout_ticks: usize,
                        keepalive_ticks: usize,
                        backoff_ticks: usize,
                        timer_client: TimerClient,
                        connector: C,
                        identity_client: IdentityClient,
                        rng: R,
                        spawner: S) -> Result<(), ChannelerError>
where
    A: Clone + Send + Sync + 'static,
    C: Connector<Address=A, SendItem=Vec<u8>, RecvItem=Vec<u8>> + Clone + Send + Sync + 'static,
    FF: Stream<Item=FunderToChanneler<A>> + Unpin,
    TF: Sink<SinkItem=ChannelerToFunder> + Unpin,
    R: CryptoRandom + 'static,
    S: Spawn + Clone + Send + Sync + 'static,
{

    let (friend_event_sender, friend_event_receiver) = mpsc::channel::<FriendEvent>(0);
    let (connections_sender, connections_receiver) = mpsc::channel::<(PublicKey, ConnPair<Vec<u8>,Vec<u8>>)>(0);

    let mut channeler = Channeler::new(connector, 
                                       to_funder,
                                       timer_client,
                                       conn_timeout_ticks,
                                       keepalive_ticks,
                                       backoff_ticks,
                                       identity_client.clone(),
                                       rng.clone(),
                                       spawner,
                                       friend_event_sender,
                                       connections_sender);


    let from_funder = from_funder
        .map(|funder_to_channeler| ChannelerEvent::FromFunder(funder_to_channeler));

    let friend_event_receiver = friend_event_receiver
        .map(|friend_event| ChannelerEvent::FriendEvent(friend_event));

    let connections_receiver = connections_receiver
        .map(|(public_key, conn_pair)| ChannelerEvent::Connection((public_key, conn_pair)));

    let mut events = from_funder
        .select(friend_event_receiver)
        .select(connections_receiver);

    while let Some(event) = await!(events.next()) {
        match event {
            ChannelerEvent::FromFunder(funder_to_channeler) => 
                await!(handle_from_funder(&mut channeler, funder_to_channeler))?,
            ChannelerEvent::Connection((public_key, conn_pair)) => { 
                let ConnPair {sender, receiver} = conn_pair;

                // We use an overwrite channel to make sure we are never stuck on trying to send a
                // message to remote friend. A friend only needs to know the most recent message,
                // so previous pending messages may be discarded.
                let (friend_sender, friend_receiver) = mpsc::channel(0);
                channeler.spawner.spawn(overwrite_send_all(sender, friend_receiver)
                              .map_err(|e| error!("overwrite_send_all() error: {:?}", e))
                              .map(|_| ()))
                    .map_err(|_| ChannelerError::SpawnError)?;


                let friend = match channeler.friends.get_mut(&public_key) {
                    None => {
                        error!("Incoming message from a non listed friend {:?}", public_key);
                        continue;
                    },
                    Some(friend) => friend,
                };
                if let FriendState::Connected(_) = &friend.state {
                    error!("Friend is already connected!");
                    continue;
                }

                let friend_connected = FriendConnected { 
                    opt_sender: Some(friend_sender),
                };
                friend.state = FriendState::Connected(friend_connected);

                let c_public_key = public_key.clone();
                let mut receiver = receiver
                    .map(move |message| FriendEvent::IncomingMessage((c_public_key.clone(), message)))
                    .chain(stream::once(future::ready(
                                FriendEvent::ReceiverClosed(public_key.clone()))));
                let mut c_friend_event_sender = channeler.friend_event_sender.clone();
                let fut = async move {
                    await!(c_friend_event_sender.send_all(&mut receiver))
                }.then(|_| future::ready(()));
                channeler.spawner.spawn(fut)
                    .map_err(|_| ChannelerError::SpawnError)?;

                let message = ChannelerToFunder::Online(public_key);
                await!(channeler.to_funder.send(message))
                    .map_err(|_| ChannelerError::SendToFunderFailed)?
            },
            ChannelerEvent::FriendEvent(FriendEvent::IncomingMessage((public_key, data))) => {
                let message = ChannelerToFunder::Message((public_key, data));
                await!(channeler.to_funder.send(message))
                    .map_err(|_| ChannelerError::SendToFunderFailed)?
            },
            ChannelerEvent::FriendEvent(FriendEvent::ReceiverClosed(public_key)) => {
                let old_friend = match channeler.friends.remove(&public_key) {
                    None => {
                        error!("A non listed friend's {:?} receiver was closed.", public_key);
                        continue;
                    },
                    Some(old_friend) => {
                        let message = ChannelerToFunder::Offline(public_key.clone());
                        await!(channeler.to_funder.send(message))
                            .map_err(|_| ChannelerError::SendToFunderFailed)?;
                        old_friend
                    },
                };

                let friend = spawn_friend(&mut channeler, public_key.clone(), old_friend.opt_address)?;
                channeler.friends.insert(public_key, friend);
            },
        };
    }
    Ok(())
}

