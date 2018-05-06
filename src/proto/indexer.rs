use std::mem;
use crypto::identity::{PublicKey, Signature};
use proto::networker::NetworkerSendPrice;

pub const INDEXING_PROVIDER_ID_LEN: usize = 16;
pub const INDEXING_PROVIDER_STATE_HASH_LEN: usize = 32;

/// The identifier of an indexing provider.
define_fixed_bytes!(IndexingProviderId, INDEXING_PROVIDER_ID_LEN);

/// A hash of a full link in an indexing provider chain
define_fixed_bytes!(IndexingProviderStateHash, INDEXING_PROVIDER_STATE_HASH_LEN);

#[derive(Clone, Debug, Eq, PartialEq)]
struct NeighborRouteLink {
    node_public_key: PublicKey,
    request_payment_proposal: NetworkerSendPrice,
    response_payment_proposal: NetworkerSendPrice,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NeighborsRoute {
    source_public_key: PublicKey,
    route_links: Vec<NeighborRouteLink>,
    destination_public_key: PublicKey,
}

#[derive(PartialEq, Eq)]
pub enum PkPairPosition {
    NotFound,
    NotLast,
    IsLast,
}

impl NeighborRouteLink {
    pub fn bytes_count() -> usize {
        mem::size_of::<PublicKey>() + 
            NetworkerSendPrice::bytes_count() * 2
    }
}

impl NeighborsRoute {
    pub fn bytes_count(&self) -> usize {
        mem::size_of::<PublicKey>() * 2 +
            NeighborRouteLink::bytes_count()
                * self.route_links.len()
    }
}
/*
    /// Find two consecutive public keys (pk1, pk2) inside a neighbors route.
    /// If found, returns the index of the first of them.
    pub fn find_pk_pair(&self, pk1: &PublicKey, pk2: &PublicKey) -> PkPairPosition {
        let public_keys = &self.public_keys;
        for i in 1 .. public_keys.len() {
            if &public_keys[i] == pk2 && &public_keys[i-1] == pk1 {
                if i == public_keys.len() - 1 {
                    return PkPairPosition::IsLast;
                } else {
                    return PkPairPosition::NotLast;
                }
            }
        }
        PkPairPosition::NotFound
    }

    pub fn is_unique(&self) -> bool {
        let public_keys = &self.public_keys;
        for i in 0 .. public_keys.len() {
            for j in i + 1.. public_keys.len() {
                if public_keys[i] == public_keys[j]{
                    return false
                }
            }
        }
        true
    }

    fn index_of(&self, key: &PublicKey) -> Option<usize>{
        self.public_keys.iter().position(|k| k == key)
    }

    pub fn get_destination_public_key(&self) -> Option<PublicKey>{
        let key = self.public_keys.last()?;
        Some(key.clone())
    }

    pub fn distance_between_nodes(&self, first_node: &PublicKey, second_node: &PublicKey)
        -> Option<usize>{
        let index_first = self.index_of(first_node)?;
        let index_second = self.index_of(second_node)?;
        if index_first > index_second{
            None
        }else{
            Some(index_second - index_first)
        }
    }
}
*/

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IndexerRoute {
    pub neighbors_route: NeighborsRoute,
    pub app_port: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FriendsRouteWithCapacity {
    pub public_keys: Vec<PublicKey>,
    // How much credit can we push through this route?
    pub capacity: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StateChainLink {
    pub previous_state_hash: IndexingProviderStateHash,
    pub new_owners_public_keys: Vec<PublicKey>,
    pub new_indexers_public_keys: Vec<PublicKey>,
    pub signatures_by_old_owners: Vec<Signature>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RequestNeighborsRoutes {
    pub source_node_public_key: PublicKey,
    pub destination_node_public_key: PublicKey,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResponseNeighborsRoutes {
    pub routes: Vec<NeighborsRoute>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResponseFriendsRoutes {
    pub routes: Vec<FriendsRouteWithCapacity>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResponseUpdateState {
    pub state_hash: IndexingProviderStateHash,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RequestUpdateState {
    pub indexing_provider_id: IndexingProviderId,
    pub indexing_provider_states_chain: Vec<StateChainLink>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RoutesToIndexer {
    pub indexing_provider_id: IndexingProviderId,
    pub routes: Vec<IndexerRoute>,
    pub request_price: u64,
}
