use std::collections::{HashMap, VecDeque};
use futures::{StreamExt, SinkExt};
use futures::task::{SpawnError, Spawn, SpawnExt};
use futures::channel::{oneshot, mpsc};

use crypto::identity::PublicKey;
use proto::index_client::messages::{IndexMutation, UpdateFriend};

pub type FriendsMap = HashMap<PublicKey, (u128, u128)>;

pub struct SeqFriends {
    friends: HashMap<PublicKey, (u128, u128)>,
    friends_queue: VecDeque<PublicKey>,
    cycle_countdown: usize,
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

impl SeqFriends {
    pub fn new(friends: FriendsMap) -> Self {
        let friends_queue = friends
            .iter()
            .map(|(public_key, _)| public_key.clone())
            .collect::<VecDeque<_>>();

        SeqFriends {
            friends,
            friends_queue,
            cycle_countdown: friends_queue.len(),
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

    pub fn reset_countdown(&mut self) {
        self.cycle_countdown = self.friends_queue.len();
    }

    /// Return information of some current friend.
    ///
    /// Should return all friends after about n calls, where n is the amount of friends.
    /// This is important as the index server relies on this behaviour. If some friend is not
    /// returned after a large amount of calls, it will be deleted from the server.
    pub fn next_update(&mut self) -> Option<(usize, UpdateFriend)> {
        self.friends_queue.pop_front().map(|friend_public_key| {
            // Move to the end of the queue:
            self.friends_queue.push_back(friend_public_key.clone());

            let (send_capacity, recv_capacity) = 
                self.friends.get(&friend_public_key).unwrap().clone();

            let update_friend = UpdateFriend {
                public_key: friend_public_key,
                send_capacity,
                recv_capacity,
            };

            let cycle_countdown = self.cycle_countdown;
            self.cycle_countdown = self.cycle_countdown.saturating_sub(1);
            (cycle_countdown, update_friend)
        })
    }
}


enum SeqFriendsRequest {
    Mutate(IndexMutation, oneshot::Sender<()>),
    ResetCountdown(oneshot::Sender<()>),
    NextUpdate(oneshot::Sender<Option<(usize, UpdateFriend)>>),
}

async fn seq_friends_loop(mut seq_friends: SeqFriends,
                          requests_receiver: mpsc::Receiver<SeqFriendsRequest>) {

    while let Some(request) = await!(requests_receiver.next()) {
        match request {
            SeqFriendsRequest::Mutate(index_mutation, response_sender) => {
                let _ = response_sender.send(seq_friends.mutate(&index_mutation));
            },
            SeqFriendsRequest::ResetCountdown(response_sender) => {
                let _ = response_sender.send(seq_friends.reset_countdown());
            },
            SeqFriendsRequest::NextUpdate(response_sender) => {
                let _ = response_sender.send(seq_friends.next_update());
            },
        }
    }
}



#[derive(Debug)]
pub enum SeqFriendsClientError {
    SendRequestError,
    RecvResponseError,
}

#[derive(Clone)]
pub struct SeqFriendsClient {
    requests_sender: mpsc::Sender<SeqFriendsRequest>,
}


impl SeqFriendsClient {
    pub fn new(requests_sender: mpsc::Sender<SeqFriendsRequest>) -> Self {
        SeqFriendsClient {
            requests_sender,
        }
    }

    pub async fn mutate(&mut self, index_mutation: IndexMutation) 
        -> Result<(), SeqFriendsClientError> {

        let (sender, receiver) = oneshot::channel();
        let request = SeqFriendsRequest::Mutate(index_mutation, sender);
        await!(self.requests_sender.send(request))
            .map_err(|_| SeqFriendsClientError::SendRequestError)?;
        Ok(await!(receiver)
           .map_err(|_| SeqFriendsClientError::RecvResponseError)?)
    }

    pub async fn reset_countdown(&mut self)
        -> Result<(), SeqFriendsClientError> {

        let (sender, receiver) = oneshot::channel();
        let request = SeqFriendsRequest::ResetCountdown(sender);
        await!(self.requests_sender.send(request))
            .map_err(|_| SeqFriendsClientError::SendRequestError)?;
        Ok(await!(receiver)
           .map_err(|_| SeqFriendsClientError::RecvResponseError)?)
    }

    pub async fn next_update(&mut self)
        -> Result<Option<(usize, UpdateFriend)>, SeqFriendsClientError> {

        let (sender, receiver) = oneshot::channel();
        let request = SeqFriendsRequest::NextUpdate(sender);
        await!(self.requests_sender.send(request))
            .map_err(|_| SeqFriendsClientError::SendRequestError)?;
        Ok(await!(receiver)
           .map_err(|_| SeqFriendsClientError::RecvResponseError)?)
    }
}


pub fn create_seq_friends_service<S>(seq_friends: SeqFriends,
                                    mut spawner: S) -> Result<SeqFriendsClient, SpawnError> 
where
    S: Spawn,
{
    let (requests_sender, requests_receiver) = mpsc::channel(0);
    let loop_fut = seq_friends_loop(seq_friends, requests_receiver);
    spawner.spawn(loop_fut)?;

    Ok(SeqFriendsClient::new(requests_sender))
}
