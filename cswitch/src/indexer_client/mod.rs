
fn create_indexer_client<S: NetworkerSenderClientTrait>(
    handle: &Handle,
    networker_sender_client: S,
    networker_receiver: mpsc::Receiver<NetworkerToIndexerClient>,
    plugin_manager_receiver: mpsc::Receiver<PluginManagerToIndexerClient>,
    plugin_manager_sender: mpsc::Sender<IndexerClientToPluginManager>,
    pather_receiver: mpsc::Receiver<PatherToIndexerClient>,
    pather_sender: mpsc::Sender<IndexerClientToPather>,
    funder_receiver: mpsc::Funder<FunderToIndexerClient>,
    funder_sender: mpsc::Funder<IndexerClientToFunder>)
        -> (CloseHandle, IndexerClient) 
{
    // TODO: 
    // - Possibly Create a nice interface for Funder and Pather to request routes.
    // 

}

