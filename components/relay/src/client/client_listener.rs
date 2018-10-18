use std::collections::HashSet;
use crypto::identity::PublicKey;

use timer::TimerClient;
use super::connector::Connector;

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

pub enum ClientListenerError {
}

pub async fn client_listener<C>(mut connector: C,
                                timer_client: TimerClient) -> Result<(), ClientListenerError>
where
    C: Connector<Address=(), SendItem=Vec<u8>, RecvItem=Vec<u8>>,
{
    let conn_pair = match await!(connector.connect(())) {
        Some(conn_pair) => conn_pair,
        None => return Ok(()),
    };
    Ok(())

    // TODO: 
    // - Create a new connection to the relay.
    // - Send an Init::Listen message, to turn the connection into a listen connection.

}

