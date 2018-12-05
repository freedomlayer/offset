use std::marker::Unpin;
use std::collections::HashMap;

use futures::{select, future, FutureExt, TryFutureExt, stream, Stream, StreamExt, Sink, SinkExt};
use futures::task::{Spawn, SpawnExt};
use futures::channel::{oneshot, mpsc};

use proto::funder::messages::{FunderToChanneler, ChannelerToFunder};

use crypto::identity::PublicKey;

use common::conn::{Listener, Connector, ConnPair};

use relay::client::access_control::{AccessControl, AccessControlOp};

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
    #[allow(unused)]
    close_sender: oneshot::Sender<()>,
}

enum FriendState {
    Connected(FriendConnected),
    Initiating(FriendInitiating),
    Listening,
}

struct Listening {
    access_control_sender: mpsc::Sender<AccessControlOp>,
}

enum ListenState {
    Listening(Listening), 
    Idle,
}

struct Channeler<A,C,L,S,TF> {
    friends: HashMap<PublicKey, Friend<A>>,
    listen_state: ListenState,
    connector: C,
    listener: L,
    spawner: S,
    to_funder: TF,
    friend_event_sender: mpsc::Sender<FriendEvent>,
    connections_sender: mpsc::Sender<(PublicKey, ConnPair<Vec<u8>, Vec<u8>>)>,
}

impl<A,C,L,S,TF> Channeler<A,C,L,S,TF> 
where
    A: Clone + Send + Sync + 'static,
    C: Connector<Address=(A, PublicKey), SendItem=Vec<u8>, RecvItem=Vec<u8>> + Clone + Send + Sync + 'static,
    L: Listener<Connection=(PublicKey, ConnPair<Vec<u8>, Vec<u8>>), Config=AccessControlOp, Arg=(A, AccessControl)> + Clone + Send,
    S: Spawn + Clone + Send + Sync + 'static,
    TF: Sink<SinkItem=ChannelerToFunder> + Send + Unpin,
{
    fn new(connector: C, 
           listener: L,
           spawner: S,
           to_funder: TF,
           friend_event_sender: mpsc::Sender<FriendEvent>,
           connections_sender: mpsc::Sender<(PublicKey, ConnPair<Vec<u8>, Vec<u8>>)>) -> Channeler<A,C,L,S,TF> {
        

        Channeler { 
            friends: HashMap::new(),
            listen_state: ListenState::Idle,
            connector,
            listener,
            spawner,
            to_funder,
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

fn spawn_friend<A,C,L,S,TF>(channeler: &mut Channeler<A,C,L,S,TF>,
                              public_key: PublicKey, opt_address: Option<A>) 
    -> Result<Friend<A>, ChannelerError>
where
    A: Clone + Send + Sync + 'static,
    C: Connector<Address=(A, PublicKey), SendItem=Vec<u8>, RecvItem=Vec<u8>> + Clone + Send + Sync + 'static,
    L: Listener<Connection=(PublicKey, ConnPair<Vec<u8>, Vec<u8>>), Config=AccessControlOp, Arg=(A, AccessControl)> + Clone + Send,
    S: Spawn + Clone + Send + Sync + 'static,
    TF: Sink<SinkItem=ChannelerToFunder> + Send + Unpin,
{
    let friend_state = match opt_address.clone() {
        Some(address) => {

            let mut c_connector = channeler.connector.clone();
            let mut c_connections_sender = channeler.connections_sender.clone();

            let (close_sender, close_receiver) = oneshot::channel::<()>();

            let cancellable_fut = async move {
                let connect_fut = c_connector.connect((address, public_key.clone()));
                let select_res = select! {
                    _close_receiver = close_receiver.fuse() => None,
                    connect_fut = connect_fut.fuse() => Some(connect_fut.unwrap())
                };
                match select_res {
                    Some(conn_pair) => {
                        await!(c_connections_sender.send((public_key.clone(), conn_pair)))
                            .map_err(|_| unreachable!());
                    },
                    None => {/* Canceled */},
                };
            };

            channeler.spawner.spawn(cancellable_fut)
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

async fn handle_from_funder<A,C,L,S,TF>(channeler: &mut Channeler<A,C,L,S,TF>,
                         funder_to_channeler: FunderToChanneler<A>) 
    -> Result<(), ChannelerError>  
where
    A: Clone + Send + Sync + 'static,
    C: Connector<Address=(A, PublicKey), SendItem=Vec<u8>, RecvItem=Vec<u8>> + Clone + Send + Sync + 'static,
    L: Listener<Connection=(PublicKey, ConnPair<Vec<u8>, Vec<u8>>), Config=AccessControlOp, Arg=(A, AccessControl)> + Clone + Send,
    S: Spawn + Clone + Send + Sync + 'static,
    TF: Sink<SinkItem=ChannelerToFunder> + Send + Unpin,
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
                    let c_listener = channeler.listener.clone();
                    let (access_control_sender, mut connections_receiver) = 
                        c_listener.listen((address, channeler.create_access_control()));
                    let mut c_connections_sender = channeler.connections_sender.clone();
                    channeler.spawner.spawn(async move {
                        await!(c_connections_sender.send_all(&mut connections_receiver).map(|_| ()))
                    });

                    let listening = Listening {
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
async fn channeler_loop<FF,TF,A,C,L,S>(
                        from_funder: FF, 
                        to_funder: TF,
                        connector: C,
                        listener: L,
                        spawner: S) -> Result<(), ChannelerError>
where
    FF: Stream<Item=FunderToChanneler<A>> + Unpin,
    TF: Sink<SinkItem=ChannelerToFunder> + Send + Unpin,
    A: Clone + Send + Sync + 'static,
    C: Connector<Address=(A, PublicKey), SendItem=Vec<u8>, RecvItem=Vec<u8>> + Clone + Send + Sync + 'static,
    L: Listener<Connection=(PublicKey, ConnPair<Vec<u8>, Vec<u8>>), Config=AccessControlOp, Arg=(A, AccessControl)> + Clone + Send,
    S: Spawn + Clone + Send + Sync + 'static,
{

    let (friend_event_sender, friend_event_receiver) = mpsc::channel::<FriendEvent>(0);
    let (connections_sender, connections_receiver) = mpsc::channel::<(PublicKey, ConnPair<Vec<u8>,Vec<u8>>)>(0);

    let mut channeler = Channeler::new(connector, 
                                       listener,
                                       spawner,
                                       to_funder,
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


#[cfg(test)]
mod tests {
    /*
    use super::*;
    use futures::executor::ThreadPool;

    use crypto::test_utils::DummyRandom;
    use crypto::identity::{SoftwareEd25519Identity,
                            generate_pkcs8_key_pair, PUBLIC_KEY_LEN,
                            PublicKey};
    use identity::create_identity;
    use timer::create_timer_incoming;
    use crypto::crypto_rand::RngContainer;

    async fn task_channeler_loop_basic(spawner: impl Spawn + Clone) {

        // Create a mock time service:
        let (tick_sender, tick_receiver) = mpsc::channel::<()>(0);
        let timer_client = create_timer_incoming(tick_receiver, spawner.clone()).unwrap();

        let rng = RngContainer::new(DummyRandom::new(&[1u8]));
        let pkcs8 = generate_pkcs8_key_pair(&rng);
        let identity = SoftwareEd25519Identity::from_pkcs8(&pkcs8).unwrap();
        let (requests_sender, identity_server) = create_identity(identity);
        let identity_client = IdentityClient::new(requests_sender);

        let conn_timeout_ticks = 16;
        let keepalive_ticks = 8;
        let backoff_ticks = 2;

        let (funder_sender, from_funder) = mpsc::channel(0);
        let (to_funder, funder_receiver) = mpsc::channel(0);

        channeler_loop(from_funder,
                        to_funder,
                        conn_timeout_ticks,
                        keepalive_ticks,
                        backoff_ticks,
                        timer_client.clone(),
                        connector: C,
                        identity_client,
                        rng,
                        spawner.clone())
    }

    #[test]
    fn test_channeler_loop_basic() {
        let thread_pool = ThreadPool::new().unwrap();
        thread_pool.spawn(task_channeler_loop_basic(thread_pool.clone()));
    }
    */
}
