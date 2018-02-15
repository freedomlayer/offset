use futures::{Future, Sink};
use futures::sync::{mpsc, oneshot};

use crypto::identity::PublicKey;
use networker::messages::{NetworkerToDatabase, NeighborInfo, InNeighborToken, OutNeighborToken};
use database::messages::ResponseLoadNeighborToken;
use super::super::messages::ResponseLoadNeighbors;

#[derive(Debug)]
pub enum DBNetworkerClientError {
    RequestSendFailed,
    OneshotReceiverCanceled,
}


/// A Database client used by the Networker. This client allows the Networker to store/load
/// neighbors information.
#[derive(Clone)]
pub struct DBNetworkerClient {
    requests_sender: mpsc::Sender<NetworkerToDatabase>,
}

impl DBNetworkerClient {
    /// Create a new DBNetworkerClient from a sender of requests.
    pub fn new(requests_sender: mpsc::Sender<NetworkerToDatabase>) -> Self {
        DBNetworkerClient { requests_sender }
    }

    /// Send a command to the Database. 
    /// Returns a Future that is resolved after the command was sent.
    fn send_command(&self, request: NetworkerToDatabase) -> 
        impl Future<Item=(), Error=DBNetworkerClientError> {
        let rsender = self.requests_sender.clone();
        rsender
         .send(request)
         .map_err(|_| DBNetworkerClientError::RequestSendFailed)
         .and_then(|_| Ok(()))
    }

    /// Send a request to the Database, expecting a response.
    /// Returns a Future that is resolved to the response received from the Database.
    fn request_response<R>(&self, request: NetworkerToDatabase, rx: oneshot::Receiver<R>) -> 
        impl Future<Item=R, Error=DBNetworkerClientError> {
        self.requests_sender
            .clone()
            .send(request)
            .map_err(|_| DBNetworkerClientError::RequestSendFailed)
            .and_then(|_| rx.map_err(|oneshot::Canceled| DBNetworkerClientError::OneshotReceiverCanceled))
    }

    /// Store a neighbor into the database.
    pub fn store_neighbor(&self, neighbor_info: NeighborInfo) 
        -> impl Future<Item=(), Error=DBNetworkerClientError> {
        let request = NetworkerToDatabase::StoreNeighbor(neighbor_info);
        self.send_command(request)
    }

    /// Remove a neighbor from the database according to his public key.
    pub fn remove_neighbor(&self, neighbor_public_key: PublicKey)
        -> impl Future<Item=(), Error=DBNetworkerClientError> {
        let request = NetworkerToDatabase::RemoveNeighbor {neighbor_public_key};
        self.send_command(request)
    }

    /// Load a list of all known neighbors. Retreives only small amount of information
    /// (NeighborInfo) about every neighbor. Additional information could be obtained by calling
    /// request_load_neighbor_token method with the neighbor public key.
    pub fn request_load_neighbors(&self)
        -> impl Future<Item=Vec<NeighborInfo>, Error=DBNetworkerClientError> {

        let (tx, rx) = oneshot::channel();
        let request = NetworkerToDatabase::RequestLoadNeighbors {response_sender: tx};
        self.request_response(request, rx)
         .and_then(|ResponseLoadNeighbors {neighbors}| {
             Ok(neighbors)
         })
    }

    /// Store an incoming neighbor token. 
    pub fn store_in_neighbor_token(&self, in_neighbor_token: InNeighborToken)
        -> impl Future<Item=(), Error=DBNetworkerClientError> {
        let request = NetworkerToDatabase::StoreInNeighborToken(in_neighbor_token);
        self.send_command(request)
    }

    /// Store an incoming outgoing neighbor token.
    pub fn store_out_neighbor_token(&self, out_neighbor_token: OutNeighborToken)
        -> impl Future<Item=(), Error=DBNetworkerClientError> {
        let request = NetworkerToDatabase::StoreOutNeighborToken(out_neighbor_token);
        self.send_command(request)
    }

    /// Request all known information about a neighbor token channel, given a neighbor public key
    /// and the token channel index.
    /// Note that the item returned might be None. This happens if no such token channel was found.
    /// This method usually be called only once, on the startup of the Networker.
    pub fn request_load_neighbor_token(&self, 
                                       neighbor_public_key: PublicKey, 
                                       token_channel_index: u32)
        -> impl Future<Item=Option<ResponseLoadNeighborToken>, Error=DBNetworkerClientError> {

        let (tx, rx) = oneshot::channel();
        let request = NetworkerToDatabase::RequestLoadNeighborToken {
            neighbor_public_key,
            token_channel_index,
            response_sender: tx
        };
        self.request_response(request, rx)
    }
}


