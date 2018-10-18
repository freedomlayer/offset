use std::collections::HashSet;
use futures::{future, Stream, StreamExt, Sink, SinkExt};

use crypto::identity::PublicKey;
use proto::relay::messages::{InitConnection, RelayListenOut, IncomingConnection};
use proto::relay::serialize::{serialize_init_connection,
    serialize_relay_listen_in, deserialize_relay_listen_out};

use timer::TimerClient;
use super::connector::{Connector, ConnPair};

enum AllowedOp {
    Clear,
    Add(PublicKey),
    Remove(PublicKey),
    AllowAll,
}

enum InnerAllowed {
    Only(HashSet<PublicKey>),
    All,
}

struct Allowed {
    inner: InnerAllowed,
}

#[derive(Debug)]
struct ApplyOpError;

impl Allowed {
    pub fn new() -> Allowed {
        Allowed {
            inner: InnerAllowed::Only(HashSet::new()) 
        }
    }

    pub fn apply_op(&mut self, allowed_op: AllowedOp) -> Result<(), ApplyOpError> {
        match allowed_op {
            AllowedOp::Clear => self.inner = InnerAllowed::Only(HashSet::new()),
            AllowedOp::Add(public_key) => {
                match self.inner {
                    InnerAllowed::Only(ref mut only_set) => {
                        only_set.insert(public_key);
                    },
                    InnerAllowed::All => return Err(ApplyOpError),
                }
            }
            AllowedOp::Remove(public_key) => {
                match self.inner {
                    InnerAllowed::Only(ref mut only_set) => {
                        only_set.remove(&public_key);
                    },
                    InnerAllowed::All => return Err(ApplyOpError),
                }
            }
            AllowedOp::AllowAll => self.inner = InnerAllowed::All,
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
}

enum ClientListenerEvent {
    TimerTick,

}

pub async fn client_listener<C, IAO>(mut connector: C,
                                incoming_allowed: IAO,
                                timer_client: TimerClient) -> Result<(), ClientListenerError>
where
    C: Connector<Address=(), SendItem=Vec<u8>, RecvItem=Vec<u8>>,
    IAO: Stream<Item=AllowedOp>,
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

    let mut allowed = Allowed::new();

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

