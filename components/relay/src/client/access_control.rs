use crypto::identity::PublicKey;
use std::collections::HashSet;

#[derive(Clone, Debug)]
pub enum AccessControlOp {
    Add(PublicKey),
    Remove(PublicKey),
}

pub struct AccessControl {
    allowed: HashSet<PublicKey>,
}

#[derive(Debug)]
pub struct ApplyOpError;

impl AccessControl {
    pub fn new() -> AccessControl {
        AccessControl {
            allowed: HashSet::new(),
        }
    }

    pub fn apply_op(&mut self, allowed_op: AccessControlOp) -> Result<(), ApplyOpError> {
        match allowed_op {
            AccessControlOp::Add(public_key) => {
                self.allowed.insert(public_key);
            }
            AccessControlOp::Remove(public_key) => {
                self.allowed.remove(&public_key);
            }
        }
        Ok(())
    }

    /// Check if a certain public key is allowed.
    pub fn is_allowed(&self, public_key: &PublicKey) -> bool {
        self.allowed.contains(public_key)
    }
}


// TODO: Add tests.
