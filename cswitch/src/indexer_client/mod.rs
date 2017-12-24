#![allow(unused)]
use futures::{Future, Poll, Async};
use futures::sync::mpsc;

use tokio_core::reactor::Handle;

use inner_messages::{
    NetworkerToIndexerClient,
    FromTimer,
    PluginManagerToIndexerClient,
    IndexerClientToPluginManager,
    FunderToIndexerClient,
    IndexerClientToFunder,
};

use close_handle::CloseHandle;
use networker::networker_client::NetworkerSenderClientTrait;

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

fn create_indexer_client<S: NetworkerSenderClientTrait, R>(
    handle: &Handle,
    networker_sender_client: S,
    networker_receiver: mpsc::Receiver<NetworkerToIndexerClient<R>>,
    timer_receiver: mpsc::Receiver<FromTimer>,
    plugin_manager_receiver: mpsc::Receiver<PluginManagerToIndexerClient>,
    plugin_manager_sender: mpsc::Sender<IndexerClientToPluginManager>,
    funder_receiver: mpsc::Receiver<FunderToIndexerClient>,
    funder_sender: mpsc::Sender<IndexerClientToFunder>
) -> (CloseHandle, IndexerClient) {
    // TODO: 
    // - Possibly Create a nice interface for Funder and Networker to request routes.

    unimplemented!()
}

