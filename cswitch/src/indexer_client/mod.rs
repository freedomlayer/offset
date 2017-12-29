#![allow(unused)]
use futures::{Future, Poll, Async};
use futures::sync::mpsc;
use tokio_core::reactor::Handle;

use inner_messages::{
    IndexerClientToNetworker,
    NetworkerToIndexerClient,
    FromTimer,
    AppManagerToIndexerClient,
    IndexerClientToAppManager,
    FunderToIndexerClient,
    IndexerClientToFunder,
    DatabaseToIndexerClient,
    IndexerClientToDatabase,
};

use close_handle::CloseHandle;

#[derive(Debug)]
pub struct IndexerClient {}

#[derive(Debug)]
pub enum IndexerClientError {}

impl Future for IndexerClient {
    type Item = ();
    type Error = IndexerClientError;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        unimplemented!()
    }
}

fn create_indexer_client(
    handle: &Handle,
    networker_sender: mpsc::Sender<IndexerClientToNetworker>,
    networker_receiver: mpsc::Receiver<NetworkerToIndexerClient>,
    timer_receiver: mpsc::Receiver<FromTimer>,
    app_manager_receiver: mpsc::Receiver<AppManagerToIndexerClient>,
    app_manager_sender: mpsc::Sender<IndexerClientToAppManager>,
    funder_receiver: mpsc::Receiver<FunderToIndexerClient>,
    funder_sender: mpsc::Sender<IndexerClientToFunder>,
    database_receiver: mpsc::Receiver<DatabaseToIndexerClient>,
    database_sender: mpsc::Sender<IndexerClientToDatabase>
) -> (CloseHandle, IndexerClient) {
    unimplemented!()
}
