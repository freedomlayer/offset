use std::collections::HashSet;
use futures::{future, Stream, StreamExt, Sink, SinkExt};
use futures::task::{Spawn, SpawnExt};
use futures::channel::mpsc;

use crypto::identity::PublicKey;
use proto::relay::messages::{InitConnection, RelayListenOut, IncomingConnection};
use proto::relay::serialize::{serialize_init_connection,
    serialize_relay_listen_in, deserialize_relay_listen_out};

use timer::TimerClient;
use super::connector::{Connector, ConnPair};

#[derive(Clone, Debug)]
enum AccessControlOp {
    Clear,
    Add(PublicKey),
    Remove(PublicKey),
    AllowAll,
}

enum InnerAccessControl {
    Only(HashSet<PublicKey>),
    All,
}

struct AccessControl {
    inner: InnerAccessControl,
}

#[derive(Debug)]
struct ApplyOpError;

impl AccessControl {
    pub fn new() -> AccessControl {
        AccessControl {
            inner: InnerAccessControl::Only(HashSet::new()) 
        }
    }

    pub fn apply_op(&mut self, allowed_op: AccessControlOp) -> Result<(), ApplyOpError> {
        match allowed_op {
            AccessControlOp::Clear => self.inner = InnerAccessControl::Only(HashSet::new()),
            AccessControlOp::Add(public_key) => {
                match self.inner {
                    InnerAccessControl::Only(ref mut only_set) => {
                        only_set.insert(public_key);
                    },
                    InnerAccessControl::All => return Err(ApplyOpError),
                }
            }
            AccessControlOp::Remove(public_key) => {
                match self.inner {
                    InnerAccessControl::Only(ref mut only_set) => {
                        only_set.remove(&public_key);
                    },
                    InnerAccessControl::All => return Err(ApplyOpError),
                }
            }
            AccessControlOp::AllowAll => self.inner = InnerAccessControl::All,
        }
        Ok(())
    }
}

#[derive(Debug)]
pub enum ClientListenerError {
    RequestTimerStreamError,
    SendInitConnectionError,
    ConnectionFailure,
    ConnectionTimedOut,
    TimerClosed,
}


#[derive(Debug, Clone)]
enum ClientListenerEvent {
    TimerTick,
    TimerClosed,
    AccessControlOp(AccessControlOp),
    AccessControlClosed,
    ServerMessage(RelayListenOut),
    ServerClosed,
}

async fn inner_client_listener<C,IAC,CS,CSE>(mut connector: C,
                                incoming_access_control: IAC,
                                connections_sender: CS,
                                mut timer_client: TimerClient,
                                spawner: impl Spawn,
                                opt_event_sender: Option<mpsc::Sender<ClientListenerEvent>>) 
    -> Result<(), ClientListenerError>
where
    C: Connector<Address=(), SendItem=Vec<u8>, RecvItem=Vec<u8>>,
    IAC: Stream<Item=AccessControlOp>,
    CS: Sink<SinkItem=ConnPair<Vec<u8>, Vec<u8>>, SinkError=CSE>
{
    let timer_stream = await!(timer_client.request_timer_stream())
        .map_err(|_|  ClientListenerError::RequestTimerStreamError)?;

    let conn_pair = match await!(connector.connect(())) {
        Some(conn_pair) => conn_pair,
        None => return Err(ClientListenerError::ConnectionFailure),
    };

    let ConnPair {mut sender, receiver} = conn_pair;
    let ser_init_connection = serialize_init_connection(&InitConnection::Listen);

    await!(sender.send(ser_init_connection))
        .map_err(|_| ClientListenerError::SendInitConnectionError)?;

    // Add serialization for sender:
    let mut sender = sender
        .sink_map_err(|_| ())
        .with(|vec| -> future::Ready<Result<_,()>> {
            future::ready(Ok(serialize_relay_listen_in(&vec)))
        });

    // Add deserialization for receiver:
    let mut receiver = receiver.map(|relay_listen_in| {
        deserialize_relay_listen_out(&relay_listen_in).ok()
    }).take_while(|opt_relay_listen_in| {
        future::ready(opt_relay_listen_in.is_some())
    }).map(|opt| opt.unwrap());

    let mut access_control = AccessControl::new();

    // TODO: Select all streams here, to create a main stream of events:

    while let Some(relay_listen_out) = await!(receiver.next()) {
        match relay_listen_out {
            RelayListenOut::KeepAlive => {},
            RelayListenOut::IncomingConnection(IncomingConnection(public_key)) => {},
        }
    }

    Ok(())

    // TODO: 
    // - Create a new connection to the relay.
    // - Send an Init::Listen message, to turn the connection into a listen connection.

}

