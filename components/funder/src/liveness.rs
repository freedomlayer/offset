use im::hashset::HashSet as ImHashSet;

use proto::crypto::PublicKey;

#[derive(Debug, Clone, Default)]
pub struct Liveness {
    pub friends: ImHashSet<PublicKey>,
}

impl Liveness {
    pub fn new() -> Liveness {
        Liveness {
            friends: ImHashSet::new(),
        }
    }

    pub fn set_online(&mut self, public_key: PublicKey) -> bool {
        self.friends.insert(public_key).is_none()
    }

    pub fn set_offline(&mut self, public_key: &PublicKey) -> bool {
        self.friends.remove(&public_key).is_some()
    }

    pub fn is_online(&mut self, public_key: &PublicKey) -> bool {
        self.friends.contains(public_key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_liveness_basic() {
        let mut liveness = Liveness::new();
        let pk_a = PublicKey::from(&[0xaa; PublicKey::len()]);
        let pk_b = PublicKey::from(&[0xbb; PublicKey::len()]);
        let pk_c = PublicKey::from(&[0xcc; PublicKey::len()]);

        assert!(!liveness.is_online(&pk_a));
        assert!(!liveness.is_online(&pk_b));
        assert!(!liveness.is_online(&pk_c));

        liveness.set_online(pk_a.clone());
        assert!(liveness.is_online(&pk_a));
        assert!(!liveness.is_online(&pk_b));
        assert!(!liveness.is_online(&pk_c));

        liveness.set_online(pk_a.clone());
        assert!(liveness.is_online(&pk_a));
        assert!(!liveness.is_online(&pk_b));
        assert!(!liveness.is_online(&pk_c));

        liveness.set_online(pk_b.clone());
        assert!(liveness.is_online(&pk_a));
        assert!(liveness.is_online(&pk_b));
        assert!(!liveness.is_online(&pk_c));

        liveness.set_offline(&pk_c);
        assert!(liveness.is_online(&pk_a));
        assert!(liveness.is_online(&pk_b));
        assert!(!liveness.is_online(&pk_c));

        liveness.set_offline(&pk_b);
        assert!(liveness.is_online(&pk_a));
        assert!(!liveness.is_online(&pk_b));
        assert!(!liveness.is_online(&pk_c));

        liveness.set_offline(&pk_b);
        assert!(liveness.is_online(&pk_a));
        assert!(!liveness.is_online(&pk_b));
        assert!(!liveness.is_online(&pk_c));

        liveness.set_offline(&pk_a);
        assert!(!liveness.is_online(&pk_a));
        assert!(!liveness.is_online(&pk_b));
        assert!(!liveness.is_online(&pk_c));
    }
}
