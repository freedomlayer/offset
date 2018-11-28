use std::marker::Unpin;
use std::collections::HashMap;

use futures::{future, FutureExt, TryFutureExt, stream, Stream, StreamExt, Sink, SinkExt};
use futures::task::{Spawn, SpawnExt};
use futures::channel::mpsc;

use proto::funder::messages::{FunderToChanneler, ChannelerToFunder};
use crypto::identity::PublicKey;
use timer::TimerClient;

use utils::int_convert::usize_to_u64;

use relay::client::connector::{Connector, ConnPair};
use relay::client::client_listener::{client_listener, ClientListenerError};
use relay::client::client_connector::{ClientConnector};
use relay::client::access_control::AccessControlOp;

use crate::listener::listener_loop;

pub enum ChannelerEvent<A> {
    FromFunder(FunderToChanneler<A>),
    IncomingConnection((PublicKey, ConnPair<Vec<u8>, Vec<u8>>)),
    ConnectionEstablished(()),
    IncomingMessage((PublicKey, Vec<u8>)),
    FriendReceiverClosed((PublicKey, u64)), // (friend_public_key, generation)
}

pub enum ChannelerError {
    SpawnError,
    SendToFunderFailed,
    AddressSendFailed,
}

pub struct ConnectedFriend {
    sender: mpsc::Sender<Vec<u8>>,
    // generation is used to avoid problems of delayed removal.
    // As ConnectedFriend could be removed due to various reasons (Failed send, closed receiver),
    // we fear the possibility of an old removal command removing a newer generation
    // ConnectedFriend. Having a generation field allows to mitigate this problem.
    generation: u64,
}

pub struct ChannelerState<A> {
    friends: HashMap<PublicKey, ConnectedFriend>,
    addresses_sender: mpsc::Sender<A>,
    access_control_sender: mpsc::Sender<AccessControlOp>,
    event_sender: mpsc::Sender<ChannelerEvent<A>>,
    generation: u64,
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
            generation: 0,
        }
    }
}

async fn handle_from_funder<A>(channeler_state: &mut ChannelerState<A>, 
                         funder_to_channeler: FunderToChanneler<A>) 
    -> Result<(), ChannelerError>  {

    match funder_to_channeler {
        FunderToChanneler::Message((public_key, message)) => {
            let mut connected_friend = match channeler_state.friends.remove(&public_key) {
                Some(connected_friend) => connected_friend,
                None => {
                    error!("Attempt to send a message to unavailable friend: {:?}", public_key);
                    return Ok(());
                },
            };
            if let Ok(()) = await!(connected_friend.sender.send(message)) {
                channeler_state.friends.insert(public_key, connected_friend);
            }             
            // Note: In a case of failure the friend is removed from the connected friends map.
            Ok(())
        },
        FunderToChanneler::SetAddress(address) =>
            await!(channeler_state.addresses_sender.send(address))
                .map_err(|_| ChannelerError::AddressSendFailed),
        FunderToChanneler::AddFriend((public_key, opt_address)) => unimplemented!(),
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


    let listener_loop_fut = listener_loop(connector.clone(),
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
            error!("[Channeler] listener_loop() error: {:?}", e);
        }).then(|_| future::ready(()));

    spawner.spawn(listener_loop_fut)
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

                let generation = channeler_state.generation;
                let connected_friend = ConnectedFriend { 
                    sender, 
                    generation,
                };
                channeler_state.friends.insert(public_key.clone(), connected_friend);
                channeler_state.generation = channeler_state.generation.wrapping_add(1);

                let c_public_key = public_key.clone();
                let mut receiver = receiver
                    .map(move |message| ChannelerEvent::IncomingMessage((c_public_key.clone(), message)))
                    .chain(stream::once(future::ready(
                                ChannelerEvent::FriendReceiverClosed((public_key.clone(), generation)))));
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
            ChannelerEvent::FriendReceiverClosed((public_key, generation)) => {
                if let Some(connected_friend) = channeler_state.friends.get(&public_key) {
                    if connected_friend.generation != generation {
                        return Ok(());
                    }
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


