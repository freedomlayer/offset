use std::net::SocketAddr;
use im::hashmap::HashMap as ImHashMap;

use num_bigint::BigUint;
use num_traits::identities::Zero;

use crypto::identity::PublicKey;
// use crypto::rand_values::RandValue;

use super::friend::{FriendState, FriendMutation};


#[allow(unused)]
#[derive(Clone)]
pub struct MessengerState {
    pub local_public_key: PublicKey,
    pub incoming_path_fee: u64,
    pub friends: ImHashMap<PublicKey, FriendState>,
}

#[allow(unused)]
pub enum MessengerMutation {
    FriendMutation((PublicKey, FriendMutation)),
    AddFriend((PublicKey, Option<SocketAddr>, u16)),
    // (friend_public_key, friend_addr, max_channels)
    RemoveFriend(PublicKey),
    SetIncomingPathFee(u64),
}


#[allow(unused)]
impl MessengerState {
    pub fn new() -> MessengerState {
        // TODO: Initialize from database somehow.
        unreachable!();
    }

    /// Get total trust (in credits) we put on all the friends together.
    pub fn get_total_trust(&self) -> BigUint {
        let mut sum: BigUint = BigUint::zero();
        for friend in self.friends.values() {
            sum += friend.get_trust();
        }
        sum
    }

    pub fn get_friends(&self) -> &ImHashMap<PublicKey, FriendState> {
        &self.friends
    }

    pub fn get_local_public_key(&self) -> &PublicKey {
        &self.local_public_key
    }

    pub fn mutate(&mut self, messenger_mutation: &MessengerMutation) {
        match messenger_mutation {
            MessengerMutation::FriendMutation((public_key, friend_mutation)) => {
                let friend = self.friends.get_mut(&public_key).unwrap();
                friend.mutate(friend_mutation);
            },
            MessengerMutation::AddFriend((friend_public_key, opt_socket_addr, max_channels)) => {
                let friend = FriendState::new(&self.local_public_key,
                                                  friend_public_key,
                                                  *opt_socket_addr, 
                                                  *max_channels);
                // Insert friend, but also make sure that we did not remove any existing friend
                // with the same public key:
                let _ = self.friends.insert(friend_public_key.clone(), friend).unwrap();

            },
            MessengerMutation::RemoveFriend(public_key) => {
                let _ = self.friends.remove(&public_key);
            },
            MessengerMutation::SetIncomingPathFee(incoming_path_fee) => {
                self.incoming_path_fee = *incoming_path_fee;
            },
        }

    }
}
