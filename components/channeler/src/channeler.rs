use std::marker::Unpin;
use futures::{future, FutureExt, TryFutureExt, Stream, StreamExt, Sink};
use futures::task::{Spawn, SpawnExt};
use futures::channel::mpsc;

use proto::funder::messages::{FunderToChanneler, ChannelerToFunder};
use crypto::identity::PublicKey;
use timer::TimerClient;

use utils::int_convert::usize_to_u64;

use relay::client::connector::{Connector, ConnPair};
use relay::client::client_listener::{client_listener, ClientListenerError};
use relay::client::client_connector::{ClientConnector};

use crate::listener::listener_loop;

pub enum ChannelerEvent<A> {
    FromFunder(FunderToChanneler<A>),
    IncomingConnection(()),
    ConnectionEstablished(()),
}

pub enum ChannelerError {
    SpawnError,
}

fn inner_channeler_loop<FF,TF,C,A,S>(address: A,
                        from_funder: FF, 
                        to_funder: TF,
                        conn_timeout_ticks: usize,
                        keepalive_ticks: usize,
                        backoff_ticks: usize,
                        timer_client: TimerClient,
                        connector: C,
                        mut spawner: S) -> Result<(), ChannelerError>
where
    A: Clone + Send + Sync + 'static,
    C: Connector<Address=A, SendItem=Vec<u8>, RecvItem=Vec<u8>> + Clone + Send + Sync + 'static,
    FF: Stream<Item=FunderToChanneler<A>>,
    TF: Sink<SinkItem=ChannelerToFunder>,
    S: Spawn + Clone + Send + Sync + 'static,
{

    let client_connector = ClientConnector::new(connector.clone(), 
                                                spawner.clone(), 
                                                timer_client.clone(), 
                                                keepalive_ticks);

    let (addresses_sender, addresses_receiver) = mpsc::channel(0);
    let (access_control_sender, access_control_receiver) = mpsc::channel(0);
    let (connections_sender, connections_receiver) = mpsc::channel(0);

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
            error!("[Channeler] inner_listener_loop() error: {:?}", e);
        }).then(|_| future::ready(()));

    spawner.spawn(listener_loop_fut)
        .map_err(|_| ChannelerError::SpawnError)?;



    unimplemented!();
    // TODO:
    // Handle events in a loop:
    // - from Funder:
    //      - Message((PublicKey, Vec<u8>)), // (friend_public_key, message)
    //      - SetAddress(A), 
    //      - AddFriend((PublicKey, A)), // (friend_public_key, address)
    //      - RemoveFriend(PublicKey), // friend_public_key
    // - Incoming connection (from listener)
    // - Connection established (We initiated this connection)
}


