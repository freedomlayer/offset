use std::collections::{HashMap, HashSet};
use std::hash::Hash;

pub struct Relay<P, ST> {
    pub friends: HashSet<P>,
    pub status: ST,
}

pub struct ListenPoolState<RA, P, ST> {
    pub relays: HashMap<RA, Relay<P, ST>>,
    local_addresses: HashSet<RA>,
}

impl<RA, P, ST> ListenPoolState<RA, P, ST>
where
    RA: Hash + Eq + Clone,
    P: Hash + Eq + Clone,
{
    pub fn new() -> Self {
        ListenPoolState {
            relays: HashMap::new(),
            local_addresses: HashSet::new(),
        }
    }

    /// Set a new list for local_addresses.
    /// Returns directions to spawn relays (relay_friends, relay_addresses)
    /// For each address in relay_addresses, we should spawn a new relay with relay_friends as
    /// friends.
    pub fn set_local_addresses(&mut self, local_addresses: Vec<RA>) -> (HashSet<P>, Vec<RA>) {
        self.local_addresses = local_addresses.iter().cloned().collect::<HashSet<_>>();

        // Remove relays that we don't need to listen to anymore:
        // This should disconnect from those relays automatically:
        self.relays.retain(|relay_address, relay| {
            if !relay.friends.is_empty() {
                return true;
            }
            local_addresses.contains(relay_address)
        });

        // Start listening to new relays if necessary:
        let mut new_addresses = Vec::new();
        for address in &self.local_addresses {
            if !self.relays.contains_key(address) {
                new_addresses.push(address.clone());
            }
        }

        // Local chosen relays should allow all friends to connect:
        let mut relay_friends = HashSet::new();
        for relay in self.relays.values() {
            for friend in &relay.friends {
                relay_friends.insert(friend.clone());
            }
        }

        (relay_friends, new_addresses)
    }

    /// Update relays related to a friend (Possibly adding a new friend).
    /// Returns: (relays_add, relays_spawn)
    /// - relays_add is a list of relays to add the friend to.
    /// - relays_remove is a list of relays to remove the friend from.
    /// - relays_spawn is a list of new relays to spawn
    pub fn update_friend(
        &mut self,
        friend_public_key: P,
        addresses: Vec<RA>,
    ) -> (Vec<RA>, Vec<RA>, Vec<RA>) {
        let mut relays_add = Vec::new();
        let mut relays_remove = Vec::new();
        let mut relays_spawn = Vec::new();

        // We add the friend to all relevant relays (as specified by addresses),
        // but also to all our local addresses relays.
        let iter_addresses = addresses
            .iter()
            .cloned()
            .chain(self.local_addresses.iter().cloned())
            .collect::<HashSet<_>>();

        for address in iter_addresses.iter() {
            match self.relays.get_mut(address) {
                Some(relay) => {
                    if relay.friends.contains(&friend_public_key) {
                        continue;
                    }
                    relays_add.push(address.clone());
                    relay.friends.insert(friend_public_key.clone());
                }
                None => {
                    relays_spawn.push(address.clone());
                }
            }
        }

        // Relays for which we should signal for removal of this friend:
        for (relay_address, relay) in &self.relays {
            if addresses.contains(relay_address) {
                continue;
            }
            if self.local_addresses.contains(relay_address) {
                continue;
            }
            if !relay.friends.contains(&friend_public_key) {
                continue;
            }
            relays_remove.push(relay_address.clone());
        }

        let c_local_addresses = self.local_addresses.clone();

        // Removal:
        self.relays.retain(|relay_address, relay| {
            if addresses.contains(relay_address) || c_local_addresses.contains(relay_address) {
                return true;
            }
            let _ = relay.friends.remove(&friend_public_key);
            // We remove the relay connection if it was only required
            // by the removed friend:
            if !relay.friends.is_empty() {
                return true;
            }
            false
        });

        (relays_add, relays_remove, relays_spawn)
    }

    /// Remove a friend
    /// Returns: relays_remove - relays to signal friend_public_key has been removed.
    pub fn remove_friend(&mut self, friend_public_key: &P) -> Vec<RA> {
        let local_addresses = self.local_addresses.clone();

        let mut relays_remove = Vec::new();

        // Update access control:
        for (relay_address, relay) in &self.relays {
            if !relay.friends.contains(friend_public_key) {
                continue;
            }
            relays_remove.push(relay_address.clone());
        }

        self.relays.retain(|relay_address, relay| {
            // Update relay friends:
            let _ = relay.friends.remove(friend_public_key);
            // We remove the relay connection if it was only required
            // by the removed friend:
            if !relay.friends.is_empty() {
                return true;
            }
            local_addresses.contains(relay_address)
        });

        relays_remove
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    #[derive(Debug)]
    struct Status;

    /// A util struct that simulates the usage of ListenPoolState.
    struct ListenPoolStateWrap<RA, P>(ListenPoolState<RA, P, Status>);

    impl<RA, P> ListenPoolStateWrap<RA, P>
    where
        RA: Hash + Eq + Clone,
        P: Hash + Eq + Clone,
    {
        pub fn new() -> Self {
            ListenPoolStateWrap(ListenPoolState::new())
        }

        pub fn set_local_addresses(&mut self, local_addresses: Vec<RA>) -> (HashSet<P>, Vec<RA>) {
            let (friends, relay_addresses) = self.0.set_local_addresses(local_addresses);
            for address in &relay_addresses {
                let res = self.0.relays.insert(
                    address.clone(),
                    Relay {
                        friends: friends.clone(),
                        status: Status,
                    },
                );
                assert!(res.is_none());
            }
            (friends, relay_addresses)
        }

        pub fn update_friend(
            &mut self,
            friend_public_key: P,
            addresses: Vec<RA>,
        ) -> (Vec<RA>, Vec<RA>, Vec<RA>) {
            let (relays_add, relays_remove, relays_spawn) =
                self.0.update_friend(friend_public_key.clone(), addresses);

            let mut friends = HashSet::new();
            friends.insert(friend_public_key.clone());

            for address in &relays_spawn {
                let res = self.0.relays.insert(
                    address.clone(),
                    Relay {
                        friends: friends.clone(),
                        status: Status,
                    },
                );
                assert!(res.is_none());
            }
            (relays_add, relays_remove, relays_spawn)
        }

        pub fn remove_friend(&mut self, friend_public_key: &P) -> Vec<RA> {
            self.0.remove_friend(friend_public_key)
        }
    }

    #[test]
    fn test_listen_pool_state_set_local_addresses() {
        let mut lps = ListenPoolStateWrap::<u32, u64>::new();
        let (friends, mut relay_addresses) = lps.set_local_addresses(vec![0u32, 1u32, 2u32]);
        assert!(friends.is_empty());
        relay_addresses.sort();
        assert_eq!(relay_addresses, vec![0u32, 1u32, 2u32]);

        let (friends, mut relay_addresses) = lps.set_local_addresses(vec![0u32, 1u32, 3u32, 4u32]);
        assert!(friends.is_empty());
        relay_addresses.sort();
        assert_eq!(relay_addresses, vec![3u32, 4u32]);

        assert!(lps.0.relays.get(&2u32).is_none());

        let (friends, relay_addresses) = lps.set_local_addresses(vec![4u32]);
        assert!(friends.is_empty());
        assert!(relay_addresses.is_empty());
    }

    #[test]
    fn test_listen_pool_state_update_friend() {
        let mut lps = ListenPoolStateWrap::<u32, u64>::new();
        let _ = lps.set_local_addresses(vec![0u32, 1u32, 2u32]);

        let (mut relays_add, relays_remove, mut relays_spawn) =
            lps.update_friend(100u64, vec![1u32, 2u32, 3u32]);
        relays_add.sort();
        assert_eq!(relays_add, vec![0u32, 1u32, 2u32]);
        assert!(relays_remove.is_empty());
        relays_spawn.sort();
        assert_eq!(relays_spawn, vec![3u32]);

        let (mut relays_add, mut relays_remove, mut relays_spawn) =
            lps.update_friend(100u64, vec![4u32]);
        relays_add.sort();
        assert_eq!(relays_add, Vec::<u32>::new());
        relays_remove.sort();
        assert_eq!(relays_remove, vec![3u32]);
        relays_spawn.sort();
        assert_eq!(relays_spawn, vec![4u32]);

        let (mut relays_add, mut relays_remove, mut relays_spawn) =
            lps.update_friend(100u64, vec![0u32, 4u32, 5u32, 6u32]);
        relays_add.sort();
        assert!(relays_add.is_empty());
        relays_remove.sort();
        assert!(relays_remove.is_empty());
        relays_spawn.sort();
        assert_eq!(relays_spawn, vec![5u32, 6u32]);

        let (mut relays_add, mut relays_remove, mut relays_spawn) =
            lps.update_friend(200u64, vec![0u32, 3u32, 4u32, 5u32, 6u32]);
        relays_add.sort();
        assert_eq!(relays_add, vec![0u32, 1u32, 2u32, 4u32, 5u32, 6u32]);
        relays_remove.sort();
        assert!(relays_remove.is_empty());
        relays_spawn.sort();
        assert_eq!(relays_spawn, vec![3u32]);

        let (mut relays_add, mut relays_remove, mut relays_spawn) =
            lps.update_friend(300u64, vec![7u32]);
        relays_add.sort();
        assert_eq!(relays_add, vec![0u32, 1u32, 2u32]);
        relays_remove.sort();
        assert!(relays_remove.is_empty());
        relays_spawn.sort();
        assert_eq!(relays_spawn, vec![7u32]);

        let (mut relays_add, mut relays_remove, mut relays_spawn) =
            lps.update_friend(400u64, vec![7u32]);
        relays_add.sort();
        assert_eq!(relays_add, vec![0u32, 1u32, 2u32, 7u32]);
        relays_remove.sort();
        assert!(relays_remove.is_empty());
        relays_spawn.sort();
        assert!(relays_remove.is_empty());

        let (mut relays_add, mut relays_remove, mut relays_spawn) =
            lps.update_friend(300u64, vec![]);
        relays_add.sort();
        assert!(relays_add.is_empty());
        relays_remove.sort();
        assert_eq!(relays_remove, vec![7u32]);
        relays_spawn.sort();
        assert!(relays_spawn.is_empty());
    }

    #[test]
    fn test_listen_pool_state_remove_friend() {
        let mut lps = ListenPoolStateWrap::<u32, u64>::new();
        let _ = lps.set_local_addresses(vec![0u32, 1u32, 2u32]);

        let (mut relays_add, relays_remove, mut relays_spawn) =
            lps.update_friend(100u64, vec![2u32, 3u32, 4u32]);
        relays_add.sort();
        assert_eq!(relays_add, vec![0u32, 1u32, 2u32]);
        assert!(relays_remove.is_empty());
        relays_spawn.sort();
        assert_eq!(relays_spawn, vec![3u32, 4u32]);

        let mut relays_remove = lps.remove_friend(&100u64);
        relays_remove.sort();
        assert_eq!(relays_remove, vec![0u32, 1u32, 2u32, 3u32, 4u32]);

        let _ = lps.update_friend(200u64, vec![7u32]);
        let _ = lps.update_friend(300u64, vec![7u32]);

        let mut relays_remove = lps.remove_friend(&300u64);
        relays_remove.sort();
        assert_eq!(relays_remove, vec![0u32, 1u32, 2u32, 7u32]);
        assert!(lps.0.relays.get(&7u32).is_some());
    }
}
