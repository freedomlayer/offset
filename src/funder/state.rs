use im::hashmap::HashMap as ImHashMap;

use num_bigint::BigUint;
use num_traits::identities::Zero;

use crypto::identity::PublicKey;
use crypto::uid::Uid;
use super::friend::{FriendState, FriendMutation};
use proto::common::SendFundsReceipt;


#[allow(unused)]
#[derive(Clone)]
pub struct FunderState<A:Clone> {
    pub local_public_key: PublicKey,
    pub friends: ImHashMap<PublicKey, FriendState<A>>,
    pub ready_receipts: ImHashMap<Uid, SendFundsReceipt>,
}

#[allow(unused)]
pub enum FunderMutation<A> {
    FriendMutation((PublicKey, FriendMutation<A>)),
    AddFriend((PublicKey, Option<A>)), // (friend_public_key, opt_address)
    RemoveFriend(PublicKey),
}


#[allow(unused)]
impl<A:Clone> FunderState<A> {
    pub fn new() -> FunderState<A> {
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

    pub fn mutate(&mut self, messenger_mutation: &FunderMutation<A>) {
        match messenger_mutation {
            FunderMutation::FriendMutation((public_key, friend_mutation)) => {
                let friend = self.friends.get_mut(&public_key).unwrap();
                friend.mutate(friend_mutation);
            },
            FunderMutation::AddFriend((friend_public_key, opt_address)) => {
                let friend = FriendState::new(&self.local_public_key,
                                                  friend_public_key,
                                                  opt_address.clone());
                // Insert friend, but also make sure that we did not remove any existing friend
                // with the same public key:
                let _ = self.friends.insert(friend_public_key.clone(), friend).unwrap();

            },
            FunderMutation::RemoveFriend(public_key) => {
                let _ = self.friends.remove(&public_key);
            },
        }
    }
}
