use std::cmp::Ordering;
use std::marker::Unpin;
use std::collections::HashMap;

use futures::{select, future, FutureExt, TryFutureExt, stream, Stream, StreamExt, Sink, SinkExt};
use futures::task::{Spawn, SpawnExt};
use futures::channel::{oneshot, mpsc};

use proto::funder::messages::{FunderToChanneler, ChannelerToFunder};
use common::conn::{Listener, FutTransform, ConnPair};
use crypto::identity::{PublicKey, compare_public_key};

use common::access_control::{AccessControlOp, AccessControl};

use crate::overwrite_channel::overwrite_send_all;

type AccessControlPk = AccessControl<PublicKey>;
type AccessControlOpPk = AccessControlOp<PublicKey>;

#[derive(Debug)]
pub enum ChannelerEvent<A> {
    FromFunder(FunderToChanneler<A>),
    Connection((PublicKey, ConnPair<Vec<u8>, Vec<u8>>)),
    FriendEvent(FriendEvent),
}

#[derive(Debug)]
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
    address: A,
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
    access_control_sender: mpsc::Sender<AccessControlOpPk>,
}

enum ListenState {
    Listening(Listening), 
    Idle,
}

struct Channeler<A,C,L,S,TF> {
    local_public_key: PublicKey,
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
    C: FutTransform<Input=(A, PublicKey), Output=ConnPair<Vec<u8>,Vec<u8>>> + Clone + Send + Sync + 'static,
    L: Listener<Connection=(PublicKey, ConnPair<Vec<u8>, Vec<u8>>), Config=AccessControlOpPk, Arg=(A, AccessControlPk)> + Clone + Send,
    S: Spawn + Clone + Send + Sync + 'static,
    TF: Sink<SinkItem=ChannelerToFunder> + Send + Unpin,
{
    fn new(local_public_key: PublicKey,
           connector: C, 
           listener: L,
           spawner: S,
           to_funder: TF,
           friend_event_sender: mpsc::Sender<FriendEvent>,
           connections_sender: mpsc::Sender<(PublicKey, ConnPair<Vec<u8>, Vec<u8>>)>) -> Channeler<A,C,L,S,TF> {
        

        Channeler { 
            local_public_key,
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

    /// Should we wait for a connection from `friend_public_key`.
    /// In other words: Is the remote side active?
    fn is_listen_friend(&self, friend_public_key: &PublicKey) -> bool {
        compare_public_key(&self.local_public_key, friend_public_key) == Ordering::Less
    }

    /// Create AccessControlPk according to current configured friends.
    /// Only listening friends are included.
    fn create_access_control(&self) -> AccessControlPk {
        let mut access_control = AccessControlPk::new();
        for (public_key, _) in &self.friends {
            if self.is_listen_friend(public_key) {
                access_control.apply_op(AccessControlOp::Add(public_key.clone()));
            }
        }
        access_control
    }

    fn spawn_friend(&mut self,
                    public_key: PublicKey, address: A) 
        -> Result<Friend<A>, ChannelerError> {

        if self.is_listen_friend(&public_key) {
            return Ok(Friend {
                address,
                state: FriendState::Listening,
            });
        }

        let mut c_connector = self.connector.clone();
        let mut c_connections_sender = self.connections_sender.clone();

        let (close_sender, close_receiver) = oneshot::channel::<()>();

        let c_address = address.clone();
        let cancellable_fut = async move {
            let connect_fut = c_connector.transform((c_address, public_key.clone()));
            // Note: We assume that our connector never returns None (Because it will keep trying
            // forever). Therefore we may unwrap connect_fut here. 
            // Maybe we should change the design of the trait to force this behaviour.
            let select_res = select! {
                _close_receiver = close_receiver.fuse() => None,
                connect_fut = connect_fut.fuse() => Some(connect_fut)
            };
            match select_res {
                Some(conn_pair) => {
                    let _ = await!(c_connections_sender.send((public_key.clone(), conn_pair)));
                },
                None => {/* Canceled */},
            };
        };

        self.spawner.spawn(cancellable_fut)
            .map_err(|_| ChannelerError::SpawnError)?;

        let friend_state = FriendState::Initiating(FriendInitiating {
            close_sender,
        });

        Ok(Friend {
            address,
            state: friend_state,
        })
    }
}


async fn handle_from_funder<A,C,L,S,TF>(channeler: &mut Channeler<A,C,L,S,TF>,
                         funder_to_channeler: FunderToChanneler<A>) 
    -> Result<(), ChannelerError>  
where
    A: Clone + Send + Sync + 'static,
    C: FutTransform<Input=(A, PublicKey), Output=ConnPair<Vec<u8>,Vec<u8>>> + Clone + Send + Sync + 'static,
    L: Listener<Connection=(PublicKey, ConnPair<Vec<u8>, Vec<u8>>), Config=AccessControlOpPk, Arg=(A, AccessControlPk)> + Clone + Send,
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
                    }).map_err(|_| ChannelerError::SpawnError)?;

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
        FunderToChanneler::AddFriend((public_key, address)) => {
            if channeler.friends.contains_key(&public_key) {
                error!("Friend {:?} already exists! Aborting.", public_key);
                return Ok(());
            }
            let friend = channeler.spawn_friend(public_key.clone(), address.clone())?;
            channeler.friends.insert(public_key.clone(), friend);

            if channeler.is_listen_friend(&public_key) {
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
                Some(_) => {
                    if channeler.is_listen_friend(&public_key) {
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
                        local_public_key: PublicKey,
                        from_funder: FF, 
                        to_funder: TF,
                        connector: C,
                        listener: L,
                        spawner: S) -> Result<(), ChannelerError>
where
    FF: Stream<Item=FunderToChanneler<A>> + Unpin,
    TF: Sink<SinkItem=ChannelerToFunder> + Send + Unpin,
    A: Clone + Send + Sync + 'static + std::fmt::Debug,
    C: FutTransform<Input=(A, PublicKey), Output=ConnPair<Vec<u8>,Vec<u8>>> + Clone + Send + Sync + 'static,
    L: Listener<Connection=(PublicKey, ConnPair<Vec<u8>, Vec<u8>>), Config=AccessControlOpPk, Arg=(A, AccessControlPk)> + Clone + Send,
    S: Spawn + Clone + Send + Sync + 'static,
{

    let (friend_event_sender, friend_event_receiver) = mpsc::channel::<FriendEvent>(0);
    let (connections_sender, connections_receiver) = mpsc::channel::<(PublicKey, ConnPair<Vec<u8>,Vec<u8>>)>(0);

    let mut channeler = Channeler::new(local_public_key, 
                                       connector, 
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
                let (sender, receiver) = conn_pair;

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

                let friend = channeler.spawn_friend(public_key.clone(), old_friend.address.clone())?;
                channeler.friends.insert(public_key, friend);
            },
        };
    }
    Ok(())
}


#[cfg(test)]
mod tests {
    use super::*;
    use futures::executor::ThreadPool;

    use common::dummy_connector::DummyConnector;
    use common::dummy_listener::DummyListener;
    use crypto::identity::{PublicKey, PUBLIC_KEY_LEN};

    /// Test the case of a friend the channeler initiates connection to.
    async fn task_channeler_loop_connect_friend<S>(mut spawner: S)
    where
        S: Spawn + Clone + Send + Sync + 'static,
    {

        let (mut funder_sender, from_funder) = mpsc::channel(0);
        let (to_funder, mut funder_receiver) = mpsc::channel(0);

        // We sort the public keys ahead of time, so that we know how to break ties.
        // Our local public key will be pks[1]. pks[0] < pks[1] < pks[2]
        //
        // pks[1] >= pks[0], so pks[0] be an active send friend (We initiate connection)
        // pks[1] < pks[2], hence pks[2] will be a listen friend. (We wait for him to connect)
        let mut pks = (0 .. 3)
            .map(|i| PublicKey::from(&[i; PUBLIC_KEY_LEN]))
            .collect::<Vec<PublicKey>>();
        pks.sort_by(compare_public_key);


        let (conn_request_sender, mut conn_request_receiver) = mpsc::channel(0);
        let connector = DummyConnector::new(conn_request_sender);

        let (listener_req_sender, mut listener_req_receiver) = mpsc::channel(0);
        let listener = DummyListener::new(listener_req_sender, spawner.clone());


        spawner.spawn(channeler_loop(pks[1].clone(),
                       from_funder,
                       to_funder,
                       connector,
                       listener,
                       spawner.clone())
            .map_err(|e| error!("Error in channeler_loop(): {:?}", e))
            .map(|_| ())).unwrap();

        // Play with changing relay addresses:
        await!(funder_sender.send(FunderToChanneler::SetAddress(Some(0x1337u32)))).unwrap();
        let listener_request = await!(listener_req_receiver.next()).unwrap();
        let (ref address, _) = listener_request.arg;
        assert_eq!(address, &0x1337u32);
        // Empty relay address:
        await!(funder_sender.send(FunderToChanneler::SetAddress(None))).unwrap();

        // This is the final address we set for our relay:
        await!(funder_sender.send(FunderToChanneler::SetAddress(Some(0x1u32)))).unwrap();
        let listener_request = await!(listener_req_receiver.next()).unwrap();
        let (ref address, _) = listener_request.arg;
        assert_eq!(address, &0x1u32);

        // Add a friend:
        await!(funder_sender.send(FunderToChanneler::AddFriend((pks[0].clone(), 0x0u32)))).unwrap();
        let conn_request = await!(conn_request_receiver.next()).unwrap();
        assert_eq!(conn_request.address, (0x0u32, pks[0].clone()));

        let (mut pk0_sender, remote_receiver) = mpsc::channel(0);
        let (remote_sender, mut pk0_receiver) = mpsc::channel(0);
        conn_request.reply((remote_sender, remote_receiver));

        // Friend should be reported as online:
        let channeler_to_funder = await!(funder_receiver.next()).unwrap();
        match channeler_to_funder {
            ChannelerToFunder::Online(public_key) => assert_eq!(public_key, pks[0]),
            _ => unreachable!(),
        };

        // Send a message to pks[0]:
        await!(funder_sender.send(FunderToChanneler::Message((pks[0].clone(), vec![1,2,3])))).unwrap();
        assert_eq!(await!(pk0_receiver.next()).unwrap(), vec![1,2,3]);

        // Send a message from pks[0]:
        await!(pk0_sender.send(vec![3,2,1])).unwrap();

        // We expect to get the message from pks[0]:
        let channeler_to_funder = await!(funder_receiver.next()).unwrap();
        match channeler_to_funder {
            ChannelerToFunder::Message((public_key, message)) => {
                assert_eq!(public_key, pks[0]);
                assert_eq!(message, vec![3,2,1]);
            },
            _ => unreachable!(),
        };

        // Drop pks[0] connection:
        drop(pk0_sender);
        drop(pk0_receiver);

        // pks[0] should be reported as offline:
        let channeler_to_funder = await!(funder_receiver.next()).unwrap();
        match channeler_to_funder {
            ChannelerToFunder::Offline(public_key) => assert_eq!(public_key, pks[0]),
            _ => unreachable!(),
        };

        // Connection to pks[0] should be attempted again:
        let conn_request = await!(conn_request_receiver.next()).unwrap();
        assert_eq!(conn_request.address, (0x0u32, pks[0].clone()));

        let (pk0_sender, remote_receiver) = mpsc::channel(0);
        let (remote_sender, pk0_receiver) = mpsc::channel(0);
        conn_request.reply((remote_sender, remote_receiver));

        // Online report:
        let channeler_to_funder = await!(funder_receiver.next()).unwrap();
        match channeler_to_funder {
            ChannelerToFunder::Online(public_key) => assert_eq!(public_key, pks[0]),
            _ => unreachable!(),
        };

        // Drop pks[0] connection:
        drop(pk0_sender);
        drop(pk0_receiver);

        // Offline report:
        let channeler_to_funder = await!(funder_receiver.next()).unwrap();
        match channeler_to_funder {
            ChannelerToFunder::Offline(public_key) => assert_eq!(public_key, pks[0]),
            _ => unreachable!(),
        };

        // Remove friend:
        await!(funder_sender.send(FunderToChanneler::RemoveFriend(pks[0].clone()))).unwrap();
    }

    #[test]
    fn test_channeler_loop_connect_friend() {
        let mut thread_pool = ThreadPool::new().unwrap();
        thread_pool.run(task_channeler_loop_connect_friend(thread_pool.clone()));
    }

    /// Test the case of the channeler waiting for a connection from a friend.
    async fn task_channeler_loop_listen_friend<S>(mut spawner: S)
    where
        S: Spawn + Clone + Send + Sync + 'static,
    {

        let (mut funder_sender, from_funder) = mpsc::channel(0);
        let (to_funder, mut funder_receiver) = mpsc::channel(0);

        // We sort the public keys ahead of time, so that we know how to break ties.
        // Our local public key will be pks[1]. pks[0] < pks[1] < pks[2]
        //
        // pks[1] >= pks[0], so pks[0] be an active send friend (We initiate connection)
        // pks[1] < pks[2], hence pks[2] will be a listen friend. (We wait for him to connect)
        let mut pks = (0 .. 3)
            .map(|i| PublicKey::from(&[i; PUBLIC_KEY_LEN]))
            .collect::<Vec<PublicKey>>();
        pks.sort_by(compare_public_key);


        let (conn_request_sender, _conn_request_receiver) = mpsc::channel(0);
        let connector = DummyConnector::new(conn_request_sender);

        let (listener_req_sender, mut listener_req_receiver) = mpsc::channel(0);
        let listener = DummyListener::new(listener_req_sender, spawner.clone());


        spawner.spawn(channeler_loop(pks[1].clone(),
                       from_funder,
                       to_funder,
                       connector,
                       listener,
                       spawner.clone())
            .map_err(|e| error!("Error in channeler_loop(): {:?}", e))
            .map(|_| ())).unwrap();

        // Set address for our relay:
        await!(funder_sender.send(FunderToChanneler::SetAddress(Some(0x1u32)))).unwrap();
        let mut listener_request = await!(listener_req_receiver.next()).unwrap();
        let (ref address, _) = listener_request.arg;
        assert_eq!(address, &0x1u32);

        // Add a friend:
        await!(funder_sender.send(FunderToChanneler::AddFriend((pks[2].clone(), 0x2u32)))).unwrap();

        let access_control_op = await!(listener_request.config_receiver.next()).unwrap();
        assert_eq!(access_control_op, AccessControlOp::Add(pks[2].clone()));

        // Set up connection, exchange messages and close the connection a few times:
        for _ in 0 .. 3 {
            // The channeler now listens. It waits for an incoming connection from pks[2]
            // Set up a connection from pks[2]:
            let (mut pk2_sender, receiver) = mpsc::channel(0);
            let (sender, mut pk2_receiver) = mpsc::channel(0);
            await!(listener_request.conn_sender.send((pks[2].clone(), (sender, receiver)))).unwrap();

            // Friend should be reported as online:
            let channeler_to_funder = await!(funder_receiver.next()).unwrap();
            match channeler_to_funder {
                ChannelerToFunder::Online(public_key) => assert_eq!(public_key, pks[2]),
                _ => unreachable!(),
            };

            // Send a message to pks[2]:
            await!(funder_sender.send(FunderToChanneler::Message((pks[2].clone(), vec![1,2,3])))).unwrap();
            assert_eq!(await!(pk2_receiver.next()).unwrap(), vec![1,2,3]);

            // Send a message from pks2:
            await!(pk2_sender.send(vec![3,2,1])).unwrap();

            // We expect to get the message from pks[2]:
            let channeler_to_funder = await!(funder_receiver.next()).unwrap();
            match channeler_to_funder {
                ChannelerToFunder::Message((public_key, message)) => {
                    assert_eq!(public_key, pks[2]);
                    assert_eq!(message, vec![3,2,1]);
                },
                _ => unreachable!(),
            };

            // Drop pks[2] connection:
            drop(pk2_sender);
            drop(pk2_receiver);

            // Friend should be reported as offline:
            let channeler_to_funder = await!(funder_receiver.next()).unwrap();
            match channeler_to_funder {
                ChannelerToFunder::Offline(public_key) => assert_eq!(public_key, pks[2]),
                _ => unreachable!(),
            };
        }

        // Remove friend:
        await!(funder_sender.send(FunderToChanneler::RemoveFriend(pks[2].clone()))).unwrap();
    }

    #[test]
    fn test_channeler_loop_listen_friend() {
        let mut thread_pool = ThreadPool::new().unwrap();
        thread_pool.run(task_channeler_loop_listen_friend(thread_pool.clone()));
    }


    // TODO: Add tests to make sure access control works properly?
    // If a friend with a strange public key tries to connect, he should not be able to succeed?
}
