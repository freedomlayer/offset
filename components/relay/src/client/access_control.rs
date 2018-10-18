use crypto::identity::PublicKey;
use std::collections::HashSet;

#[derive(Clone, Debug)]
pub enum AccessControlOp {
    Clear,
    Add(PublicKey),
    Remove(PublicKey),
    AllowAll,
}

enum InnerAccessControl {
    Only(HashSet<PublicKey>),
    All,
}

pub struct AccessControl {
    inner: InnerAccessControl,
}

#[derive(Debug)]
pub struct ApplyOpError;

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


// TODO: Add tests.
