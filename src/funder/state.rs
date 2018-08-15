use im::hashmap::HashMap as ImHashMap;

use num_bigint::BigUint;
use num_traits::identities::Zero;

use crypto::identity::PublicKey;
use super::friend::{FriendState, FriendMutation};


#[allow(unused)]
#[derive(Clone)]
pub struct MessengerState<A:Clone> {
    pub local_public_key: PublicKey,
    pub friends: ImHashMap<PublicKey, FriendState<A>>,
}

#[allow(unused)]
pub enum MessengerMutation<A> {
    FriendMutation((PublicKey, FriendMutation)),
    AddFriend((PublicKey, Option<A>)), // (friend_public_key, opt_address)
    RemoveFriend(PublicKey),
}


#[allow(unused)]
impl<A:Clone> MessengerState<A> {
    pub fn new() -> MessengerState<A> {
        // TODO: Initialize from database somehow.
        unreachable!();
    }

    /// Get total trust (in credits) we put on all the friends together.
    pub fn get_total_trust(&self) -> BigUint {
        let mut sum: BigUint = BigUint::zero();
        for friend in self.friends.values() {
            let remote_max_debt: BigUint = friend.directional.token_channel.state().balance.remote_max_debt.into();
            sum += remote_max_debt;
        }
        sum
    }

    pub fn get_friends(&self) -> &ImHashMap<PublicKey, FriendState<A>> {
        &self.friends
    }

    pub fn get_local_public_key(&self) -> &PublicKey {
        &self.local_public_key
    }

    pub fn mutate(&mut self, messenger_mutation: &MessengerMutation<A>) {
        match messenger_mutation {
            MessengerMutation::FriendMutation((public_key, friend_mutation)) => {
                let friend = self.friends.get_mut(&public_key).unwrap();
                friend.mutate(friend_mutation);
            },
            MessengerMutation::AddFriend((friend_public_key, opt_address)) => {
                let friend = FriendState::new(&self.local_public_key,
                                                  friend_public_key,
                                                  opt_address.clone());
                // Insert friend, but also make sure that we did not remove any existing friend
                // with the same public key:
                let _ = self.friends.insert(friend_public_key.clone(), friend).unwrap();

            },
            MessengerMutation::RemoveFriend(public_key) => {
                let _ = self.friends.remove(&public_key);
            },
        }
    }
}
