use std::collections::HashSet;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AccessControlOp<T> {
    Add(T),
    Remove(T),
}

#[derive(Clone, Debug)]
pub struct AccessControl<T: std::cmp::Eq + std::hash::Hash> {
    allowed: HashSet<T>,
}

impl<T> AccessControl<T> 
where
    T: std::cmp::Eq + std::hash::Hash,
{
    pub fn new() -> AccessControl<T> {
        AccessControl {
            allowed: HashSet::new(),
        }
    }

    pub fn apply_op(&mut self, allowed_op: AccessControlOp<T>) {
        match allowed_op {
            AccessControlOp::Add(item) => {
                self.allowed.insert(item);
            }
            AccessControlOp::Remove(item) => {
                self.allowed.remove(&item);
            }
        }
    }

    /// Check if a certain public key is allowed.
    pub fn is_allowed(&self, item: &T) -> bool {
        self.allowed.contains(item)
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_access_control_basic() {
        let a_public_key = 0xaa;
        let b_public_key = 0xbb;

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

