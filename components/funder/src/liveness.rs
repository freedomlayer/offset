use std::hash::Hash;
use im::hashset::HashSet as ImHashSet;
use proto::funder::messages::TPublicKey;

#[derive(Clone)]
pub struct Liveness<P: Clone> {
    pub friends: ImHashSet<TPublicKey<P>>,
}

#[derive(Debug)]
pub enum LivenessMutation<P> {
    SetOnline(TPublicKey<P>),
    SetOffline(TPublicKey<P>),
}


impl<P> Liveness<P> 
where
    P: Eq + Hash + Clone + Ord,
{
    pub fn new() -> Self {
        Liveness {
            friends: ImHashSet::new(),
        }
    }

    pub fn mutate(&mut self, mutation: &LivenessMutation<P>) {
        match mutation {
            LivenessMutation::SetOnline(public_key) => {
                self.friends.insert(public_key.clone());
            },
            LivenessMutation::SetOffline(public_key) => {
                let _ = self.friends.remove(public_key);
            },
        }
    }

    pub fn is_online(&self, friend_public_key: &TPublicKey<P>) -> bool {
        self.friends.contains(&friend_public_key)
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_liveness_basic() {
        let mut liveness = Liveness::new();
        let pk_a = TPublicKey::new(0xaau8);
        let pk_b = TPublicKey::new(0xbbu8);
        let pk_c = TPublicKey::new(0xccu8);

        assert!(!liveness.is_online(&pk_a));
        assert!(!liveness.is_online(&pk_b));
        assert!(!liveness.is_online(&pk_c));

        liveness.mutate(&LivenessMutation::SetOnline(pk_a.clone()));
        assert!(liveness.is_online(&pk_a));
        assert!(!liveness.is_online(&pk_b));
        assert!(!liveness.is_online(&pk_c));

        liveness.mutate(&LivenessMutation::SetOnline(pk_a.clone()));
        assert!(liveness.is_online(&pk_a));
        assert!(!liveness.is_online(&pk_b));
        assert!(!liveness.is_online(&pk_c));

        liveness.mutate(&LivenessMutation::SetOnline(pk_b.clone()));
        assert!(liveness.is_online(&pk_a));
        assert!(liveness.is_online(&pk_b));
        assert!(!liveness.is_online(&pk_c));

        liveness.mutate(&LivenessMutation::SetOffline(pk_c.clone()));
        assert!(liveness.is_online(&pk_a));
        assert!(liveness.is_online(&pk_b));
        assert!(!liveness.is_online(&pk_c));

        liveness.mutate(&LivenessMutation::SetOffline(pk_b.clone()));
        assert!(liveness.is_online(&pk_a));
        assert!(!liveness.is_online(&pk_b));
        assert!(!liveness.is_online(&pk_c));

        liveness.mutate(&LivenessMutation::SetOffline(pk_b.clone()));
        assert!(liveness.is_online(&pk_a));
        assert!(!liveness.is_online(&pk_b));
        assert!(!liveness.is_online(&pk_c));

        liveness.mutate(&LivenessMutation::SetOffline(pk_a.clone()));
        assert!(!liveness.is_online(&pk_a));
        assert!(!liveness.is_online(&pk_b));
        assert!(!liveness.is_online(&pk_c));
    }
}
