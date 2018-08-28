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

