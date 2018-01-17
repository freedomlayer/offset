use proto::indexer::{IndexingProviderId, NeighborsRoute, StateChainLink};
use crypto::identity::{PublicKey, Signature};

/// Indexing provider status.
pub enum IndexingProviderStatus {
    Enabled = 1,
    Disabled = 0,
}

/// The indexing provider's information.
pub struct IndexingProviderInfo {
    pub id: IndexingProviderId,
    pub state_chain_link: StateChainLink,
}

/// The indexing provider's information from database.
pub struct IndexingProviderInfoFromDB {
    id: IndexingProviderId,
    state_chain_link: StateChainLink,
    last_routes: Vec<NeighborsRoute>,
    status: IndexingProviderStatus,
}

// ========== Interface with the Database ==========

pub enum IndexerClientToDatabase {
    StoreIndexingProvider(IndexingProviderInfo),
    RemoveIndexingProvider(IndexingProviderId),
    RequestLoadIndexingProviders,
    StoreRoute {
        id: IndexingProviderId,
        route: NeighborsRoute,
    },
}

pub enum DatabaseToIndexerClient {
    ResponseLoadIndexingProviders(Vec<IndexingProviderInfoFromDB>)
}