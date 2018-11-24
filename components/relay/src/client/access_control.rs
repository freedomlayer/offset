use crypto::identity::PublicKey;
use std::collections::HashSet;

#[derive(Clone, Debug)]
pub enum AccessControlOp {
    Add(PublicKey),
    Remove(PublicKey),
}

#[derive(Clone, Debug)]
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


#[cfg(test)]
mod tests {
    use super::*;
    use crypto::identity::{PublicKey, PUBLIC_KEY_LEN};

    #[test]
    fn test_access_control_basic() {
        let a_public_key = PublicKey::from(&[0xaa; PUBLIC_KEY_LEN]);
        let b_public_key = PublicKey::from(&[0xbb; PUBLIC_KEY_LEN]);

        let mut ac = AccessControl::new();
        assert!(!ac.is_allowed(&a_public_key));
        assert!(!ac.is_allowed(&b_public_key));

        // Add a:
        ac.apply_op(AccessControlOp::Add(a_public_key.clone()));
        assert!(ac.is_allowed(&a_public_key));
        assert!(!ac.is_allowed(&b_public_key));

        // Add b:
        ac.apply_op(AccessControlOp::Add(b_public_key.clone()));
        assert!(ac.is_allowed(&a_public_key));
        assert!(ac.is_allowed(&b_public_key));

        // Remove a:
        ac.apply_op(AccessControlOp::Remove(a_public_key.clone()));
        assert!(!ac.is_allowed(&a_public_key));
        assert!(ac.is_allowed(&b_public_key));

        // Remove b:
        ac.apply_op(AccessControlOp::Remove(b_public_key.clone()));
        assert!(!ac.is_allowed(&a_public_key));
        assert!(!ac.is_allowed(&b_public_key));

        // Remove b again:
        ac.apply_op(AccessControlOp::Remove(b_public_key.clone()));
        assert!(!ac.is_allowed(&a_public_key));
        assert!(!ac.is_allowed(&b_public_key));
    }
}

