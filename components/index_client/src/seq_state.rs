use std::collections::{HashMap, VecDeque};

use crypto::identity::PublicKey;
use proto::index_client::messages::{IndexMutation, UpdateFriend};

pub type FriendsMap = HashMap<PublicKey, (u128, u128)>;

pub struct SeqIndexClientState {
    friends: HashMap<PublicKey, (u128, u128)>,
    friends_queue: VecDeque<PublicKey>,
}

fn apply_index_mutation(friends: &mut FriendsMap,
                            index_mutation: &IndexMutation) {
    match index_mutation {
        IndexMutation::UpdateFriend(update_friend) => {
            let capacity_pair = (update_friend.send_capacity, update_friend.recv_capacity);
            let _ = friends.insert(update_friend.public_key.clone(), capacity_pair.clone());
        }
        IndexMutation::RemoveFriend(public_key) => {
            let _ = friends.remove(public_key);
        },
    }
}

impl SeqIndexClientState {
    pub fn new(friends: FriendsMap) -> Self {
        let friends_queue = friends
            .iter()
            .map(|(public_key, _)| public_key.clone())
            .collect::<VecDeque<_>>();

        SeqIndexClientState {
            friends,
            friends_queue,
        }
    }

    pub fn mutate(&mut self, index_mutation: &IndexMutation) {
        apply_index_mutation(&mut self.friends, index_mutation);

        match index_mutation {
            IndexMutation::UpdateFriend(update_friend) => {
                // Put the newly updated friend in the end of the queue:
                // TODO: Possibly optimize this later. Might be slow:
                self.friends_queue.retain(|public_key| public_key != &update_friend.public_key);
                self.friends_queue.push_back(update_friend.public_key.clone());
            },
            IndexMutation::RemoveFriend(friend_public_key) => {
                // Remove from queue:
                // TODO: Possibly optimize this later. Might be slow:
                self.friends_queue.retain(|public_key| public_key != friend_public_key);
            },
        }
    }

    /// Return information of some current friend.
    ///
    /// Should return all friends after about n calls, where n is the amount of friends.
    /// This is important as the index server relies on this behaviour. If some friend is not
    /// returned after a large amount of calls, it will be deleted from the server.
    pub fn next_update(&mut self) -> Option<UpdateFriend> {
        match self.friends_queue.pop_front() {
            Some(friend_public_key) => {
                // Move to the end of the queue:
                self.friends_queue.push_back(friend_public_key.clone());

                let (send_capacity, recv_capacity) = 
                    self.friends.get(&friend_public_key).unwrap().clone();

                Some(UpdateFriend {
                    public_key: friend_public_key,
                    send_capacity,
                    recv_capacity,
                })
            },
            None => None,
        }
    }

    pub fn full_state_updates(&self) -> impl Iterator<Item=UpdateFriend> + '_ {
        self.friends_queue.iter()
            .map(move |friend_public_key| {
                let (send_capacity, recv_capacity) = 
                    self.friends.get(&friend_public_key).unwrap().clone();

                UpdateFriend {
                    public_key: friend_public_key.clone(),
                    send_capacity,
                    recv_capacity,
                }
            })
    }
}

