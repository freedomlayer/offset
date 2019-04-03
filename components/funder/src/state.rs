use im::hashmap::HashMap as ImHashMap;
use im::vector::Vector as ImVec;

use common::canonical_serialize::CanonicalSerialize;
use crypto::identity::PublicKey;
use crypto::uid::Uid;

use proto::app_server::messages::NamedRelayAddress;
use proto::funder::messages::{AddFriend, Receipt};

use crate::friend::{FriendMutation, FriendState};

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct FunderState<B: Clone> {
    pub local_public_key: PublicKey,
    /// Address of relay we are going to connect to.
    /// None means that no address was configured.
    pub relays: ImVec<NamedRelayAddress<B>>,
    pub friends: ImHashMap<PublicKey, FriendState<B>>,
    pub ready_receipts: ImHashMap<Uid, Receipt>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FunderMutation<B: Clone> {
    FriendMutation((PublicKey, FriendMutation<B>)),
    AddRelay(NamedRelayAddress<B>),
    RemoveRelay(PublicKey),
    AddFriend(AddFriend<B>),
    RemoveFriend(PublicKey),
    AddReceipt((Uid, Receipt)), //(request_id, receipt)
    RemoveReceipt(Uid),
}

impl<B> FunderState<B>
where
    B: Clone + CanonicalSerialize,
{
    pub fn new(local_public_key: PublicKey, relays: Vec<NamedRelayAddress<B>>) -> Self {
        // Convert relays into a map:
        let relays = relays.into_iter().collect();

        FunderState {
            local_public_key,
            relays,
            friends: ImHashMap::new(),
            ready_receipts: ImHashMap::new(),
        }
    }
    // TODO: Add code for initialization from database?

    // TODO: Use MutableState trait instead:
    pub fn mutate(&mut self, funder_mutation: &FunderMutation<B>) {
        match funder_mutation {
            FunderMutation::FriendMutation((public_key, friend_mutation)) => {
                let friend = self.friends.get_mut(&public_key).unwrap();
                friend.mutate(friend_mutation);
            }
            FunderMutation::AddRelay(named_relay_address) => {
                // Check for duplicates:
                self.relays.retain(|cur_named_relay_address| {
                    cur_named_relay_address.public_key != named_relay_address.public_key
                });
                self.relays.push_back(named_relay_address.clone());
                // TODO: Should check here if we have more than a constant amount of relays
            }
            FunderMutation::RemoveRelay(public_key) => {
                self.relays.retain(|cur_named_relay_address| {
                    &cur_named_relay_address.public_key != public_key
                });
            }
            FunderMutation::AddFriend(add_friend) => {
                let friend = FriendState::new(
                    &self.local_public_key,
                    &add_friend.friend_public_key,
                    add_friend.relays.clone(),
                    add_friend.name.clone(),
                    add_friend.balance,
                );
                // Insert friend, but also make sure that we didn't override an existing friend
                // with the same public key:
                let res = self
                    .friends
                    .insert(add_friend.friend_public_key.clone(), friend);
                assert!(res.is_none());
            }
            FunderMutation::RemoveFriend(public_key) => {
                let _ = self.friends.remove(&public_key);
            }
            FunderMutation::AddReceipt((uid, send_funds_receipt)) => {
                self.ready_receipts
                    .insert(uid.clone(), send_funds_receipt.clone());
            }
            FunderMutation::RemoveReceipt(uid) => {
                let _ = self.ready_receipts.remove(uid);
            }
        }
    }
}
