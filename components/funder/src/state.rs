use im::hashmap::HashMap as ImHashMap;

use num_bigint::BigUint;
use num_traits::{ToPrimitive, CheckedSub, pow};
use num_traits::identities::Zero;

use crypto::identity::PublicKey;
use crypto::uid::Uid;

use proto::funder::messages::{Ratio, SendFundsReceipt, AddFriend};

use common::int_convert::usize_to_u64;
use common::canonical_serialize::CanonicalSerialize;

use crate::friend::{FriendState, FriendMutation};


#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct FunderState<A: Clone> {
    pub local_public_key: PublicKey,
    /// Address of relay we are going to connect to.
    /// None means that no address was configured.
    pub address: A,
    pub friends: ImHashMap<PublicKey, FriendState<A>>,
    pub ready_receipts: ImHashMap<Uid, SendFundsReceipt>,
}

#[derive(Debug, Clone)]
pub enum FunderMutation<A> {
    FriendMutation((PublicKey, FriendMutation<A>)),
    SetAddress(A),
    AddFriend(AddFriend<A>), 
    RemoveFriend(PublicKey),
    AddReceipt((Uid, SendFundsReceipt)),  //(request_id, receipt)
    RemoveReceipt(Uid),
}


#[allow(unused)]
impl<A> FunderState<A> 
where
    A: CanonicalSerialize + Clone,
{
    pub fn new(local_public_key: &PublicKey, address: &A) -> FunderState<A> {
        FunderState {
            local_public_key: local_public_key.clone(),
            address: address.clone(),
            friends: ImHashMap::new(),
            ready_receipts: ImHashMap::new(),
        }
    }
    // TODO: Add code for initialization from database?
    
    /// Get total trust (in credits) we put on all the friends together.
    fn get_total_trust(&self) -> BigUint {
        let mut sum: BigUint = BigUint::zero();
        for friend in self.friends.values() {
            // Note that we care more about the wanted_remote_max_debt than the actual
            // remote_max_debt in this case. The trust is derived from what the user wants to
            // happen.
            let trust: BigUint = friend.wanted_remote_max_debt.into();
            sum += trust;
        }
        sum
    }

    /// Find out how much of the shared credits between `prev_friend` and our node we can let
    /// `next_friend` use. The result is a ratio between 0 and 1.
    ///
    /// We use the formula:
    /// `(a + (s/l)) / 2s`
    ///
    /// Where `a` is the `remote_max_debt` we assigned to `next_friend`, s is the sum of all
    /// `remote_max_debt`-s of all our friends besides `prev_friend`, plus 1. (We add 1 to avoid
    /// division by 0)
    ///
    /// l is the amount of friends we have, besides prev_friend.
    pub fn get_usable_ratio(&self, opt_prev_pk: Option<&PublicKey>, next_pk: &PublicKey) -> Ratio<u128> {

        let mut s = self.get_total_trust();
        let l = self.friends.len();
        if let Some(prev_pk) = opt_prev_pk {
            l.checked_sub(1);
            let prev_friend = self.friends.get(prev_pk).unwrap();
            let friend_trust = BigUint::from(prev_friend.wanted_remote_max_debt);
            s = s.checked_sub(&friend_trust).unwrap();
        }

        // TODO: What happens if we have only one friend?
        assert!(l > 0);
        s += BigUint::from(1u128); // Helps avoid division by zero

        let next_trust = BigUint::from(self.friends.get(next_pk).unwrap().wanted_remote_max_debt);


        // Calculate: (next_trust + (s/l)) / (2*s) 
        //
        // To lose less accuracy, we will calculate the following:
        // (2^128 * (next_trust*l + s)) / (2*s*l)
        // in the form of Ratio:
        let numerator = BigUint::from(next_trust) * BigUint::from(usize_to_u64(l).unwrap()) + &s;
        let denominator = BigUint::from(2u128) * &s * BigUint::from(usize_to_u64(l).unwrap());
        assert!(numerator <= denominator);

        let two_pow_128 = pow(BigUint::from(2u128), 128);
        let res_numerator = (two_pow_128 * numerator) / denominator;
        
        match res_numerator.to_u128() {
            Some(num) => Ratio::Numerator(num),
            None => Ratio::One,
        }
    }

    pub fn mutate(&mut self, funder_mutation: &FunderMutation<A>) {
        match funder_mutation {
            FunderMutation::FriendMutation((public_key, friend_mutation)) => {
                let friend = self.friends.get_mut(&public_key).unwrap();
                friend.mutate(friend_mutation);
            },
            FunderMutation::SetAddress(address) => {
                self.address = address.clone();
            }
            FunderMutation::AddFriend(add_friend) => {
                let friend = FriendState::new(&self.local_public_key,
                                                  &add_friend.friend_public_key,
                                                  add_friend.address.clone(),
                                                  add_friend.name.clone(),
                                                  add_friend.balance);
                // Insert friend, but also make sure that we didn't override an existing friend
                // with the same public key:
                let res = self.friends.insert(add_friend.friend_public_key.clone(), friend);
                assert!(res.is_none());

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
