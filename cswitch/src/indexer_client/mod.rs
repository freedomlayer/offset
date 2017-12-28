
fn create_indexer_client(
    handle: &Handle,
    networker_sender: mpsc::Sender<IndexerClientToNetworker>,
    networker_receiver: mpsc::Receiver<NetworkerToIndexerClient>,
    timer_receiver: mpsc::Receiver<FromTimer>,
    plugin_manager_receiver: mpsc::Receiver<PluginManagerToIndexerClient>,
    plugin_manager_sender: mpsc::Sender<IndexerClientToPluginManager>,
    funder_receiver: mpsc::Funder<FunderToIndexerClient>,
    funder_sender: mpsc::Funder<IndexerClientToFunder>)
        -> (CloseHandle, IndexerClient) 
{
}

