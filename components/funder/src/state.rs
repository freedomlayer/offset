use im::hashmap::HashMap as ImHashMap;

use crypto::identity::PublicKey;
use crypto::uid::Uid;

use proto::funder::messages::{Receipt, AddFriend};
use proto::funder::scheme::FunderScheme;

use crate::friend::{FriendState, FriendMutation};


#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct FunderState<FS:FunderScheme> {
    pub local_public_key: PublicKey,
    /// Address of relay we are going to connect to.
    /// None means that no address was configured.
    pub address: FS::NamedAddress,
    pub friends: ImHashMap<PublicKey, FriendState<FS>>,
    pub ready_receipts: ImHashMap<Uid, Receipt>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FunderMutation<FS:FunderScheme> {
    FriendMutation((PublicKey, FriendMutation<FS>)),
    SetAddress(FS::NamedAddress),
    AddFriend(AddFriend<FS>), 
    RemoveFriend(PublicKey),
    AddReceipt((Uid, Receipt)),  //(request_id, receipt)
    RemoveReceipt(Uid),
}


impl<FS> FunderState<FS> 
where
    FS: FunderScheme,
{
    pub fn new(local_public_key: &PublicKey, address: &FS::NamedAddress) -> FunderState<FS> {
        FunderState {
            local_public_key: local_public_key.clone(),
            address: address.clone(),
            friends: ImHashMap::new(),
            ready_receipts: ImHashMap::new(),
        }
    }
    // TODO: Add code for initialization from database?

    pub fn mutate(&mut self, funder_mutation: &FunderMutation<FS>) {
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
