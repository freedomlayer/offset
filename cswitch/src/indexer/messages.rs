use utils::crypto::identity::PublicKey;

use super::types::{FriendsRouteWithCapacity, IndexerRoute, IndexingProviderId, IndexingProviderStateHash,
                   NeighborsRoute, StateChainLink};

// ===== Internal interfaces =====

// TODO

// ===== External interfaces =====

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct RequestNeighborsRoutes {
    pub source_node_public_key: PublicKey,
    pub destination_node_public_key: PublicKey,
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct ResponseNeighborsRoutes {
    pub routes: Vec<NeighborsRoute>,
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub enum RequestFriendsRoutes {
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

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct ResponseFriendsRoutes {
    pub routes: Vec<FriendsRouteWithCapacity>,
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct ResponseUpdateState {
    pub state_hash: IndexingProviderStateHash,
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct RequestUpdateState {
    pub indexing_provider_id: IndexingProviderId,
    pub indexing_provider_states_chain: Vec<StateChainLink>,
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct RoutesToIndexer {
    pub indexing_provider_id: IndexingProviderId,
    pub routes: Vec<IndexerRoute>,
    pub request_price: u64,
}
