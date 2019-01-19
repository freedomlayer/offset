use std::collections::{HashMap, HashSet};
use std::hash::Hash;

pub struct Relay<P,ST> {
    pub friends: HashSet<P>,
    pub status: ST,
}

pub struct ListenPoolState<B,P,ST> {
    pub relays: HashMap<B, Relay<P,ST>>,
    local_addresses: HashSet<B>,
}

impl<B,P,ST> ListenPoolState<B,P,ST> 
where
    B: Hash + Eq + Clone,
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
    pub fn set_local_addresses(&mut self, local_addresses: Vec<B>) 
        -> (HashSet<P>, Vec<B>) {

        self.local_addresses = local_addresses
            .iter()
            .cloned()
            .collect::<HashSet<_>>();

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
        for (_relay_address, relay) in &self.relays {
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
    pub fn update_friend(&mut self, friend_public_key: P, addresses: Vec<B>)
                -> (Vec<B>, Vec<B>, Vec<B>) {

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
                },
                None => {
                    relays_spawn.push(address.clone());
                },
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
    pub fn remove_friend(&mut self, friend_public_key: &P) -> Vec<B> {
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
