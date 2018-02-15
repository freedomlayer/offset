use futures::{Future, Sink};
use futures::sync::{mpsc, oneshot};

use crypto::identity::PublicKey;
use funder::messages::{FunderToDatabase, FriendInfo, InFriendToken, OutFriendToken};
use database::messages::ResponseLoadFriendToken;
use super::super::messages::ResponseLoadFriends;

#[derive(Debug)]
pub enum DBFunderClientError {
    RequestSendFailed,
    OneshotReceiverCanceled,
}


/// A Database client used by the Funder.
/// This allows to Funder to request and store information related to friends.
#[derive(Clone)]
pub struct DBFunderClient {
    requests_sender: mpsc::Sender<FunderToDatabase>,
}

impl DBFunderClient {
    /// Create a new DBFunderClient from a requests_sender.
    pub fn new(requests_sender: mpsc::Sender<FunderToDatabase>) -> Self {
        DBFunderClient { requests_sender }
    }

    /// Send a command and don't expect a response.
    /// Returns a future that waits until the command was sent.
    fn send_command(&self, request: FunderToDatabase) -> 
        impl Future<Item=(), Error=DBFunderClientError> {
        let rsender = self.requests_sender.clone();
        rsender
         .send(request)
         .map_err(|_| DBFunderClientError::RequestSendFailed)
         .and_then(|_| Ok(()))
    }

    /// Send a request to the Funder. Returns a Future that waits for the response.
    fn request_response<R>(&self, request: FunderToDatabase, rx: oneshot::Receiver<R>) -> 
        impl Future<Item=R, Error=DBFunderClientError> {
        self.requests_sender
            .clone()
            .send(request)
            .map_err(|_| DBFunderClientError::RequestSendFailed)
            .and_then(|_| rx.map_err(|oneshot::Canceled| DBFunderClientError::OneshotReceiverCanceled))
    }

    /// Store a friend into the database
    pub fn store_friend(&self, friend_info: FriendInfo) 
        -> impl Future<Item=(), Error=DBFunderClientError> {
        let request = FunderToDatabase::StoreFriend(friend_info);
        self.send_command(request)
    }

    /// Remove a friend from the database according to his public key.
    pub fn remove_friend(&self, friend_public_key: PublicKey)
        -> impl Future<Item=(), Error=DBFunderClientError> {
        let request = FunderToDatabase::RemoveFriend {friend_public_key};
        self.send_command(request)
    }

    /// Load a list of all friends inside the database. This only retreives small amount of
    /// information (FriendInfo) for every friend. To obtain extra information about a specific
    /// neighbor, one should use the request_load_friend_token method.
    pub fn request_load_friends(&self)
        -> impl Future<Item=Vec<FriendInfo>, Error=DBFunderClientError> {

        let (tx, rx) = oneshot::channel();
        let request = FunderToDatabase::RequestLoadFriends {response_sender: tx};
        self.request_response(request, rx)
         .and_then(|ResponseLoadFriends {friends}| {
             Ok(friends)
         })
    }

    /// Store an incoming friend token.
    pub fn store_in_friend_token(&self, in_friend_token: InFriendToken)
        -> impl Future<Item=(), Error=DBFunderClientError> {
        let request = FunderToDatabase::StoreInFriendToken(in_friend_token);
        self.send_command(request)
    }

    /// Store an outgoing friend token.
    pub fn store_out_friend_token(&self, out_friend_token: OutFriendToken)
        -> impl Future<Item=(), Error=DBFunderClientError> {
        let request = FunderToDatabase::StoreOutFriendToken(out_friend_token);
        self.send_command(request)
    }

    /// Load a friend token given its public key. This will return the full state (including
    /// pending requests for both sides) of a friend. In case the friend public key was not found,
    /// a None will be returned.
    /// This method usually be called only once, on the startup of the Funder.
    pub fn request_load_friend_token(&self, friend_public_key: PublicKey)
        -> impl Future<Item=Option<ResponseLoadFriendToken>, Error=DBFunderClientError> {

        let (tx, rx) = oneshot::channel();
        let request = FunderToDatabase::RequestLoadFriendToken {
            friend_public_key,
            response_sender: tx
        };
        self.request_response(request, rx)
    }
}



