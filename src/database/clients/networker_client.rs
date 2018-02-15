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


#[derive(Clone)]
pub struct DBNetworkerClient {
    requests_sender: mpsc::Sender<NetworkerToDatabase>,
}

impl DBNetworkerClient {
    pub fn new(requests_sender: mpsc::Sender<NetworkerToDatabase>) -> Self {
        DBNetworkerClient { requests_sender }
    }

    fn send_command(&self, request: NetworkerToDatabase) -> 
        impl Future<Item=(), Error=DBNetworkerClientError> {
        let rsender = self.requests_sender.clone();
        rsender
         .send(request)
         .map_err(|_| DBNetworkerClientError::RequestSendFailed)
         .and_then(|_| Ok(()))
    }

    fn request_response<R>(&self, request: NetworkerToDatabase, rx: oneshot::Receiver<R>) -> 
        impl Future<Item=R, Error=DBNetworkerClientError> {
        self.requests_sender
            .clone()
            .send(request)
            .map_err(|_| DBNetworkerClientError::RequestSendFailed)
            .and_then(|_| rx.map_err(|oneshot::Canceled| DBNetworkerClientError::OneshotReceiverCanceled))
    }

    pub fn store_neighbor(&self, neighbor_info: NeighborInfo) 
        -> impl Future<Item=(), Error=DBNetworkerClientError> {
        let request = NetworkerToDatabase::StoreNeighbor(neighbor_info);
        self.send_command(request)
    }

    pub fn remove_neighbor(&self, neighbor_public_key: PublicKey)
        -> impl Future<Item=(), Error=DBNetworkerClientError> {
        let request = NetworkerToDatabase::RemoveNeighbor {neighbor_public_key};
        self.send_command(request)
    }

    pub fn request_load_neighbors(&self)
        -> impl Future<Item=Vec<NeighborInfo>, Error=DBNetworkerClientError> {

        let (tx, rx) = oneshot::channel();
        let request = NetworkerToDatabase::RequestLoadNeighbors {response_sender: tx};
        self.request_response(request, rx)
         .and_then(|ResponseLoadNeighbors {neighbors}| {
             Ok(neighbors)
         })
    }

    pub fn store_in_neighbor_token(&self, in_neighbor_token: InNeighborToken)
        -> impl Future<Item=(), Error=DBNetworkerClientError> {
        let request = NetworkerToDatabase::StoreInNeighborToken(in_neighbor_token);
        self.send_command(request)
    }

    pub fn store_out_neighbor_token(&self, out_neighbor_token: OutNeighborToken)
        -> impl Future<Item=(), Error=DBNetworkerClientError> {
        let request = NetworkerToDatabase::StoreOutNeighborToken(out_neighbor_token);
        self.send_command(request)
    }

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


