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

    fn apply_op(&mut self, allowed_op: AllowedOp) -> Result<(), ApplyOpError> {
        match allowed_op {
            AllowedOp::Clear => self.inner = InnerAllowed::Only(HashSet::new()),
            AllowedOp::Add(public_key) => {
                self.inner = match self.inner {
                    InnerAllowed::Only(mut only_set) => {
                        only_set.insert(public_key);
                        InnerAllowed::Only(only_set)
                    },
                    InnerAllowed::All => return Err(ApplyOpError),
                }
            }
            AllowedOp::Remove(public_key) => {
                self.inner = match self.inner {
                    InnerAllowed::Only(mut only_set) => {
                        only_set.remove(&public_key);
                        InnerAllowed::Only(only_set)
                    },
                    InnerAllowed::All => return Err(ApplyOpError),
                }
            }
            AllowedOp::AllowAll => self.inner = InnerAllowed::All,
        }
        Ok(())
    }
}

pub async fn client_listener<C>(connector: C,
                                timer_client: TimerClient) 
where
    C: Connector<Address=(), Item=Vec<u8>>,
{
    // TODO: 
    // - Create a new connection to the relay.
    // - Send an Init::Listen message, to turn the connection into a listen connection.

}

