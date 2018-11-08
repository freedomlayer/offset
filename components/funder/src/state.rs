use im::hashmap::HashMap as ImHashMap;

use num_bigint::BigUint;
use num_traits::identities::Zero;

use crypto::identity::PublicKey;
use crypto::uid::Uid;

use identity::IdentityClient;

use super::friend::{FriendState, FriendMutation};
use super::types::SendFundsReceipt;


#[allow(unused)]
#[derive(Clone, Serialize, Deserialize)]
pub struct FunderState<A:Clone> {
    pub local_public_key: PublicKey,
    pub friends: ImHashMap<PublicKey, FriendState<A>>,
    pub ready_receipts: ImHashMap<Uid, SendFundsReceipt>,
}

#[allow(unused)]
pub enum FunderMutation<A> {
    FriendMutation((PublicKey, FriendMutation<A>)),
    AddFriend((PublicKey, A)), // (friend_public_key, opt_address)
    RemoveFriend(PublicKey),
    AddReceipt((Uid, SendFundsReceipt)),  //(request_id, receipt)
    RemoveReceipt(Uid),
}


#[allow(unused)]
impl<A:Clone + 'static> FunderState<A> {
    pub fn new(local_public_key: &PublicKey) -> FunderState<A> {
        FunderState {
            local_public_key: local_public_key.clone(),
            friends: ImHashMap::new(),
            ready_receipts: ImHashMap::new(),
        }
    }

    // TODO: Add code for initialization from database?

    /// Get total trust (in credits) we put on all the friends together.
    pub fn get_total_trust(&self) -> BigUint {
        let mut sum: BigUint = BigUint::zero();
        for friend in self.friends.values() {
            let trust: BigUint = friend.wanted_remote_max_debt.into();
            sum += trust;
        }
        sum
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
            FunderMutation::AddReceipt((uid, send_funds_receipt)) => {
                self.ready_receipts.insert(uid.clone(), send_funds_receipt.clone());
            },
            FunderMutation::RemoveReceipt(uid) => {
                let _ = self.ready_receipts.remove(uid);
            },
        }
    }
}
