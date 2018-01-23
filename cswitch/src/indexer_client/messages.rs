use proto::indexer::{IndexingProviderId, NeighborsRoute, StateChainLink};
use crypto::identity::{PublicKey, Signature};

use futures::sync::mpsc;

use networker::messages::{RequestSendMessage};
// use crypto::identity::{PublicKey, Signature};

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


pub struct RequestNeighborsRoutes {
    source_node_public_key: PublicKey,
    destination_node_public_key: PublicKey,
    response_sender: mpsc::Sender<ResponseNeighborsRoutes>,

}

pub struct ResponseNeighborsRoutes {
    routes: Vec<NeighborsRoute>,
}


enum RequiredFriendsRoutes {
    Direct {
        source_node_public_key: PublicKey,
        destination_node_public_key: PublicKey,
    },
    LoopFromFriend {
        // A loop from myself through given friend, back to myself.
        // This is used for money rebalance when we owe the friend money.
        friend_public_key: PublicKey,
    },
    LoopToFriend {
        // A loop from myself back to myself through given friend.
        // This is used for money rebalance when the friend owe us money.
        friend_public_key: PublicKey,
    },
}

pub struct RequestFriendsRoutes {
    required_friends_routes: RequiredFriendsRoutes,
    response_sender: mpsc::Sender<ResponseFriendsRoutes>,
}

pub struct FriendsRouteWithCapacity {
    public_keys: Vec<PublicKey>,
    // How much credit can we push through this route?
    capacity: u64,
}

pub struct ResponseFriendsRoutes {
    routes: Vec<FriendsRouteWithCapacity>,
}


pub enum IndexerClientToAppManager {
    IndexingProviderStateUpdate(IndexingProviderStateUpdate),
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

/*
pub enum IndexerClientToFunder {
}
*/

pub enum IndexerClientToNetworker {
    RequestSendMessage(RequestSendMessage),
}
