use std::marker::Unpin;
use std::collections::HashMap;

use futures::{future, FutureExt, TryFutureExt, stream, Stream, StreamExt, Sink, SinkExt};
use futures::task::{Spawn, SpawnExt};
use futures::channel::{oneshot, mpsc};

use proto::funder::messages::{FunderToChanneler, ChannelerToFunder};
use crypto::identity::PublicKey;
use timer::TimerClient;

use utils::int_convert::usize_to_u64;

use relay::client::connector::{Connector, ConnPair};
use relay::client::client_listener::{client_listener, ClientListenerError};
use relay::client::client_connector::{ClientConnector};
use relay::client::access_control::AccessControlOp;

use crate::listen::listen_loop;
use crate::connect::connect;

pub enum ChannelerEvent<A> {
    FromFunder(FunderToChanneler<A>),
    IncomingConnection((PublicKey, ConnPair<Vec<u8>, Vec<u8>>)),
    ConnectionEstablished(()),
    IncomingMessage((PublicKey, Vec<u8>)),
    FriendReceiverClosed(PublicKey), 
}

pub enum ChannelerError {
    SpawnError,
    SendToFunderFailed,
    AddressSendFailed,
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

pub struct ChannelerState<A> {
    friends: HashMap<PublicKey, Friend<A>>,
    addresses_sender: mpsc::Sender<A>,
    access_control_sender: mpsc::Sender<AccessControlOp>,
    event_sender: mpsc::Sender<ChannelerEvent<A>>,
}

impl<A> ChannelerState<A> {
    fn new(addresses_sender: mpsc::Sender<A>,
           access_control_sender: mpsc::Sender<AccessControlOp>,
           event_sender: mpsc::Sender<ChannelerEvent<A>>) -> ChannelerState<A> {
        ChannelerState { 
            friends: HashMap::new(),
            addresses_sender,
            access_control_sender,
            event_sender,
        }
    }
}

async fn handle_from_funder<A>(channeler_state: &mut ChannelerState<A>, 
                         funder_to_channeler: FunderToChanneler<A>) 
    -> Result<(), ChannelerError>  {

    match funder_to_channeler {
        FunderToChanneler::Message((public_key, message)) => {
            let mut friend = match channeler_state.friends.get_mut(&public_key) {
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
        FunderToChanneler::SetAddress(address) =>
            await!(channeler_state.addresses_sender.send(address))
                .map_err(|_| ChannelerError::AddressSendFailed),
        FunderToChanneler::AddFriend((public_key, opt_address)) => {
            let friend_state = match opt_address {
                Some(address) => {
                    let (close_sender, close_receiver) = oneshot::channel::<()>();

                    // TODO: spawn a connect future here:
                    // TODO: Add more things into channeler_state. For example,
                    // we are missing spawner, timer_client, keepalive_ticks, backoff_ticks etc.
                    
                    unimplemented!();
                    /*
                    let connect_fut = connect(connector,
                            address,
                            public_key,
                            keepalive_ticks: usize,
                            backoff_ticks: usize,
                            timer_client,
                            close_receiver,
                            spawner: S) -> Result<ConnPair<Vec<u8>, Vec<u8>>, ConnectError>
                    */

                    FriendState::Initiating(FriendInitiating {
                        close_sender,
                    })
                },
                None => FriendState::Listening,
            };
            let friend = Friend {
                opt_address,
                state: friend_state,
            };
            // TODO: What to do if there is already a friend with the given public key?
            // Current idea: Write an error using error!, and do nothing?
            channeler_state.friends.insert(public_key, friend);
            Ok(())
        },
        FunderToChanneler::RemoveFriend(public_key) => {
            channeler_state.friends.remove(&public_key);
            Ok(())
        },
    }
}

async fn inner_channeler_loop<FF,TF,C,A,S>(address: A,
                        from_funder: FF, 
                        mut to_funder: TF,
                        conn_timeout_ticks: usize,
                        keepalive_ticks: usize,
                        backoff_ticks: usize,
                        timer_client: TimerClient,
                        connector: C,
                        mut spawner: S) -> Result<(), ChannelerError>
where
    A: Clone + Send + Sync + 'static,
    C: Connector<Address=A, SendItem=Vec<u8>, RecvItem=Vec<u8>> + Clone + Send + Sync + 'static,
    FF: Stream<Item=FunderToChanneler<A>> + Unpin,
    TF: Sink<SinkItem=ChannelerToFunder> + Unpin,
    S: Spawn + Clone + Send + Sync + 'static,
{

    let (addresses_sender, addresses_receiver) = mpsc::channel(0);
    let (access_control_sender, access_control_receiver) = mpsc::channel(0);
    let (event_sender, event_receiver) = mpsc::channel(0);
    let mut channeler_state = ChannelerState::new(addresses_sender, 
                                                  access_control_sender,
                                                  event_sender);

    let client_connector = ClientConnector::new(connector.clone(), 
                                                spawner.clone(), 
                                                timer_client.clone(), 
                                                keepalive_ticks);

    let (connections_sender, connections_receiver) = mpsc::channel::<(PublicKey, ConnPair<Vec<u8>, Vec<u8>>)>(0);


    let listen_loop_fut = listen_loop(connector.clone(),
                      address,
                      addresses_receiver,
                      access_control_receiver,
                      connections_sender,
                      conn_timeout_ticks,
                      keepalive_ticks,
                      backoff_ticks,
                      timer_client.clone(),
                      spawner.clone())
        .map_err(|e| {
            error!("[Channeler] listen_loop() error: {:?}", e);
        }).then(|_| future::ready(()));

    spawner.spawn(listen_loop_fut)
        .map_err(|_| ChannelerError::SpawnError)?;

    let from_funder = from_funder
        .map(|funder_to_channeler| ChannelerEvent::FromFunder(funder_to_channeler));

    let connections_receiver = connections_receiver
        .map(|incoming_conn| ChannelerEvent::IncomingConnection::<A>(incoming_conn));

    let mut events = from_funder
        .select(connections_receiver)
        .select(event_receiver);

    while let Some(event) = await!(events.next()) {
        match event {
            ChannelerEvent::FromFunder(funder_to_channeler) => 
                await!(handle_from_funder(&mut channeler_state, funder_to_channeler))?,
            ChannelerEvent::IncomingConnection((public_key, conn_pair)) => { 
                let ConnPair {sender, mut receiver} = conn_pair;

                let friend = match channeler_state.friends.get_mut(&public_key) {
                    None => continue,
                    Some(friend) => friend,
                };
                let friend_connected = FriendConnected { 
                    opt_sender: Some(sender),
                };
                friend.state = FriendState::Connected(friend_connected);

                let c_public_key = public_key.clone();
                let mut receiver = receiver
                    .map(move |message| ChannelerEvent::IncomingMessage((c_public_key.clone(), message)))
                    .chain(stream::once(future::ready(
                                ChannelerEvent::FriendReceiverClosed(public_key.clone()))));
                let mut c_event_sender = channeler_state.event_sender.clone();
                let fut = async move {
                    await!(c_event_sender.send_all(&mut receiver))
                }.then(|_| future::ready(()));
                spawner.spawn(fut).unwrap();

                let message = ChannelerToFunder::Online(public_key);
                await!(to_funder.send(message))
                    .map_err(|_| ChannelerError::SendToFunderFailed)?;
            },
            ChannelerEvent::ConnectionEstablished(()) => unimplemented!(),
            ChannelerEvent::IncomingMessage((public_key, data)) => {
                let message = ChannelerToFunder::Message((public_key, data));
                await!(to_funder.send(message))
                    .map_err(|_| ChannelerError::SendToFunderFailed)?;
            },
            ChannelerEvent::FriendReceiverClosed(public_key) => {
                if let Some(connected_friend) = channeler_state.friends.get(&public_key) {
                } else {
                    return Ok(())
                }

                let res = channeler_state.friends.remove(&public_key);
                assert!(res.is_none()); // We expect that we removed an existing friend
                let message = ChannelerToFunder::Offline(public_key);
                await!(to_funder.send(message))
                    .map_err(|_| ChannelerError::SendToFunderFailed)?;
            },
        };
    }

    unimplemented!();
}


