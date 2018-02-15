use futures::{Future, Sink};
use futures::sync::{mpsc, oneshot};

use crypto::identity::PublicKey;
use networker::messages::{NetworkerToDatabase, NeighborInfo};
use super::messages::ResponseLoadNeighbors;

#[derive(Debug)]
pub enum DBNetworkerClientError {
    RequestSendFailed,
    OneshotReceiverCanceled,
}

/*
pub enum NetworkerToDatabase {
    StoreNeighbor(NeighborInfo),
    RemoveNeighbor {
        neighbor_public_key: PublicKey,
    },
    RequestLoadNeighbors {
        response_sender: oneshot::Sender<ResponseLoadNeighbors>,
    },
    StoreInNeighborToken {
        neighbor_public_key: PublicKey,
        token_channel_index: u32,
        move_token_message: NeighborMoveToken,
        remote_max_debt: u64,
        local_max_debt: u64,
        remote_pending_debt: u64,
        local_pending_debt: u64,
        balance: i64,
        local_invoice_id: Option<InvoiceId>,
        remote_invoice_id: Option<InvoiceId>,
        closed_local_requests: Vec<Uid>,
        opened_remote_requests: Vec<PendingNeighborRequest>,
    },
    StoreOutNeighborToken {
        neighbor_public_key: PublicKey,
        move_token_message: NeighborMoveToken,
        remote_max_debt: u64,
        local_max_debt: u64,
        remote_pending_debt: u64,
        local_pending_debt: u64,
        balance: i64,
        local_invoice_id: Option<InvoiceId>,
        remote_invoice_id: Option<InvoiceId>,
        opened_local_requests: Vec<PendingNeighborRequest>,
        closed_remote_requests: Vec<Uid>,
    },
    RequestLoadNeighborToken {
        neighbor_public_key: PublicKey,
        token_channel_index: u32,
        response_sender: oneshot::Sender<Option<ResponseLoadNeighborToken>>,
    },
}
*/


#[derive(Clone)]
pub struct DBNetworkerClient {
    requests_sender: mpsc::Sender<NetworkerToDatabase>,
}

impl DBNetworkerClient {
    pub fn new(requests_sender: mpsc::Sender<NetworkerToDatabase>) -> Self {
        DBNetworkerClient { requests_sender }
    }
    pub fn store_neighbor(&self, neighbor_info: NeighborInfo) 
        -> impl Future<Item=(), Error=DBNetworkerClientError> {
        let rsender = self.requests_sender.clone();
        let request = NetworkerToDatabase::StoreNeighbor(neighbor_info);
        rsender
         .send(request)
         .map_err(|_| DBNetworkerClientError::RequestSendFailed)
         .and_then(|_| Ok(()))
    }

    pub fn remove_neighbor(&self, neighbor_public_key: PublicKey)
        -> impl Future<Item=(), Error=DBNetworkerClientError> {
        let rsender = self.requests_sender.clone();
        let request = NetworkerToDatabase::RemoveNeighbor {neighbor_public_key};
        rsender
         .send(request)
         .map_err(|_| DBNetworkerClientError::RequestSendFailed)
         .and_then(|_| Ok(()))
    }

    pub fn request_load_neighbors(&self)
        -> impl Future<Item=Vec<NeighborInfo>, Error=DBNetworkerClientError> {

        let rsender = self.requests_sender.clone();
        let (tx, rx) = oneshot::channel();
        let request = NetworkerToDatabase::RequestLoadNeighbors {response_sender: tx};
        rsender
         .send(request)
         .map_err(|_| DBNetworkerClientError::RequestSendFailed)
         .and_then(|_| rx.map_err(|oneshot::Canceled| DBNetworkerClientError::OneshotReceiverCanceled))
         .and_then(|ResponseLoadNeighbors {neighbors}| {
             Ok(neighbors)
         })
    }

    // TODO
    pub fn store_in_neighbor_token(&self, neighbor_public_key: PublicKey)
        -> impl Future<Item=(), Error=DBNetworkerClientError> {
        let rsender = self.requests_sender.clone();
        let request = NetworkerToDatabase::RemoveNeighbor {neighbor_public_key};
        rsender
         .send(request)
         .map_err(|_| DBNetworkerClientError::RequestSendFailed)
         .and_then(|_| Ok(()))
    }

    // TODO
    pub fn store_out_neighbor_token(&self, neighbor_public_key: PublicKey)
        -> impl Future<Item=(), Error=DBNetworkerClientError> {
        let rsender = self.requests_sender.clone();
        let request = NetworkerToDatabase::RemoveNeighbor {neighbor_public_key};
        rsender
         .send(request)
         .map_err(|_| DBNetworkerClientError::RequestSendFailed)
         .and_then(|_| Ok(()))
    }

    // TODO
    pub fn request_load_neighbor_token(&self)
        -> impl Future<Item=Vec<NeighborInfo>, Error=DBNetworkerClientError> {

        let rsender = self.requests_sender.clone();
        let (tx, rx) = oneshot::channel();
        let request = NetworkerToDatabase::RequestLoadNeighbors {response_sender: tx};
        rsender
         .send(request)
         .map_err(|_| DBNetworkerClientError::RequestSendFailed)
         .and_then(|_| rx.map_err(|oneshot::Canceled| DBNetworkerClientError::OneshotReceiverCanceled))
         .and_then(|ResponseLoadNeighbors {neighbors}| {
             Ok(neighbors)
         })
    }
}


