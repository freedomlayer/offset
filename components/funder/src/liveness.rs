use im::hashset::HashSet as ImHashSet;
use crypto::identity::PublicKey;

#[derive(Clone)]
pub struct Liveness {
    pub friends: ImHashSet<PublicKey>,
}


impl Liveness {
    pub fn new() -> Liveness {
        Liveness {
            friends: ImHashSet::new(),
        }
    }

    pub fn set_online(&mut self, friend_public_key: &PublicKey) {
        self.friends.insert(friend_public_key.clone());
    }

    pub fn set_offline(&mut self, friend_public_key: &PublicKey) {
        let _ = self.friends.remove(friend_public_key);
    }

    pub fn is_online(&self, friend_public_key: &PublicKey) -> bool {
        self.friends.contains(&friend_public_key)
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use crypto::identity::{PUBLIC_KEY_LEN};

    #[test]
    fn test_liveness_basic() {
        let mut liveness = Liveness::new();
        let pk_a = PublicKey::from(&[0xaa; PUBLIC_KEY_LEN]);
        let pk_b = PublicKey::from(&[0xbb; PUBLIC_KEY_LEN]);
        let pk_c = PublicKey::from(&[0xcc; PUBLIC_KEY_LEN]);

        assert!(!liveness.is_online(&pk_a));
        assert!(!liveness.is_online(&pk_b));
        assert!(!liveness.is_online(&pk_c));

        liveness.set_online(&pk_a);
        assert!(liveness.is_online(&pk_a));
        assert!(!liveness.is_online(&pk_b));
        assert!(!liveness.is_online(&pk_c));

        liveness.set_online(&pk_a);
        assert!(liveness.is_online(&pk_a));
        assert!(!liveness.is_online(&pk_b));
        assert!(!liveness.is_online(&pk_c));

        liveness.set_online(&pk_b);
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
