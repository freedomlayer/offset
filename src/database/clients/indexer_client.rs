use futures::{Future, Sink};
use futures::sync::{mpsc, oneshot};

use database::messages::{IndexingProviderInfoFromDB, ResponseLoadIndexingProviders};
use indexer_client::messages::{IndexerClientToDatabase, IndexingProviderInfo};
use proto::indexer::{IndexingProviderId, NeighborsRoute};

#[derive(Debug)]
pub enum DBIndexerClientError {
RequestSendFailed,
OneshotReceiverCanceled,
}

#[allow(doc_markdown)]
/// A databse client used by `IndexerClient`.
/// (Should have been called IndexerClientClient)
/// Used for storing and loading indexing providers' related information
/// from and to the Database.
#[derive(Clone)]
pub struct DBIndexerClient {
    requests_sender: mpsc::Sender<IndexerClientToDatabase>,
}

impl DBIndexerClient {
    /// Create a new DBNetworkerClient from a sender of requests.
    pub fn new(requests_sender: mpsc::Sender<IndexerClientToDatabase>) -> Self {
        DBIndexerClient { requests_sender }
    }

    /// Send a command to the Database. 
    /// Returns a Future that is resolved after the command was sent.
    fn send_command(&self, request: IndexerClientToDatabase) -> 
        impl Future<Item=(), Error=DBIndexerClientError> {
        let rsender = self.requests_sender.clone();
        rsender
         .send(request)
         .map_err(|_| DBIndexerClientError::RequestSendFailed)
         .and_then(|_| Ok(()))
    }

    /// Send a request to the Database, expecting a response.
    /// Returns a Future that is resolved to the response received from the Database.
    fn request_response<R>(&self, request: IndexerClientToDatabase, rx: oneshot::Receiver<R>) -> 
        impl Future<Item=R, Error=DBIndexerClientError> {
        self.requests_sender
            .clone()
            .send(request)
            .map_err(|_| DBIndexerClientError::RequestSendFailed)
            .and_then(|_| rx.map_err(|oneshot::Canceled| 
                                     DBIndexerClientError::OneshotReceiverCanceled))
    }

    /// Store an indexing provider into the database
    pub fn store_indexing_provider(&self, indexing_provider_info: IndexingProviderInfo) 
        -> impl Future<Item=(), Error=DBIndexerClientError> {
        let request = IndexerClientToDatabase::StoreIndexingProvider(indexing_provider_info);
        self.send_command(request)
    }

    /// Remove an indexing provider from the database according to his id
    pub fn remove_indexing_provider(&self, indexing_provider_id: IndexingProviderId) 
        -> impl Future<Item=(), Error=DBIndexerClientError> {
        let request = IndexerClientToDatabase::RemoveIndexingProvider(indexing_provider_id);
        self.send_command(request)
    }

    /// Load a vector of all indexing providers kept in the database
    pub fn request_load_indexing_providers(&self)
        -> impl Future<Item=Vec<IndexingProviderInfoFromDB>, Error=DBIndexerClientError> {
        let (tx, rx) = oneshot::channel();
        let request = IndexerClientToDatabase::RequestLoadIndexingProviders {response_sender: tx};
        self.request_response(request, rx)
         .and_then(|ResponseLoadIndexingProviders(indexing_providers)| {
             Ok(indexing_providers)
         })
    }

    /// Store a route related to an indexing provider.
    /// This is a route to one of the indexing provider's nodes.
    pub fn store_route(&self, indexing_provider_id: IndexingProviderId, route: NeighborsRoute) 
        -> impl Future<Item=(), Error=DBIndexerClientError> {
        let request = IndexerClientToDatabase::StoreRoute {
            id: indexing_provider_id,
            route,
        };
        self.send_command(request)
    }
}

