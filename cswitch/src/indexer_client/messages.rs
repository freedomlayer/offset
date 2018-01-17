use proto::indexer::{IndexingProviderId, NeighborsRoute, RequestFriendsRoutes,
                     RequestNeighborsRoutes, ResponseFriendsRoutes, ResponseNeighborsRoutes,
                     ResponseUpdateState, StateChainLink};

use networker::messages::{DiscardMessageReceived, RequestSendMessage, RespondMessageReceived};
use crypto::identity::{PublicKey, Signature};

/// Indexing provider status.
pub enum IndexingProviderStatus {
    Enabled = 1,
    Disabled = 0,
}

pub struct IndexingProviderLoaded {
    id: IndexingProviderId,
    state_chain_link: StateChainLink,
    status: IndexingProviderStatus,
}

pub enum IndexingProviderStateUpdate {
    Loaded(IndexingProviderLoaded),
    ChainLinkUpdated(IndexingProviderInfo),
}

/// The indexing provider's information.
pub struct IndexingProviderInfo {
    pub id: IndexingProviderId,
    pub state_chain_link: StateChainLink,
}

// ======== Internal interfaces ========

pub enum IndexerClientToAppManager {
    IndexingProviderStateUpdate(IndexingProviderStateUpdate),
    ResponseNeighborsRoutes(ResponseNeighborsRoutes),
    ResponseFriendsRoutes(ResponseFriendsRoutes),
}

pub enum IndexerClientToDatabase {
    StoreIndexingProvider(IndexingProviderInfo),
    RemoveIndexingProvider(IndexingProviderId),
    RequestLoadIndexingProviders,
    StoreRoute {
        id: IndexingProviderId,
        route: NeighborsRoute,
    },
}

pub enum IndexerClientToFunder {
    ResponseNeighborsRoute(ResponseNeighborsRoutes),
}

pub enum IndexerClientToNetworker {
    RequestSendMessage(RequestSendMessage),
    ResponseFriendsRoutes(ResponseFriendsRoutes),
    ResponseMessageReceived(RespondMessageReceived),
    DiscardMessageReceived(DiscardMessageReceived),
}
