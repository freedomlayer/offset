use std::convert::TryFrom;

use crypto::identity::{PublicKey, Signature};

pub const INDEXING_PROVIDER_ID_LEN: usize = 16;
pub const INDEXING_PROVIDER_STATE_HASH_LEN: usize = 32;

/// The identifier of an indexing provider.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct IndexingProviderId([u8; INDEXING_PROVIDER_ID_LEN]);

/// A hash of a full link in an indexing provider chain
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IndexingProviderStateHash([u8; INDEXING_PROVIDER_STATE_HASH_LEN]);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NeighborsRoute {
    pub public_keys: Vec<PublicKey>,
}

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

// =========== Conversions ==========

impl<'a> TryFrom<&'a [u8]> for IndexingProviderId {
    type Error = ();

    fn try_from(src: &'a [u8]) -> Result<IndexingProviderId, Self::Error> {
        if src.len() != INDEXING_PROVIDER_ID_LEN {
            Err(())
        } else {
            let mut inner = [0; INDEXING_PROVIDER_ID_LEN];
            inner.clone_from_slice(src);
            Ok(IndexingProviderId(inner))
        }
    }
}

impl AsRef<[u8]> for IndexingProviderId {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl<'a> TryFrom<&'a [u8]> for IndexingProviderStateHash {
    type Error = ();

    fn try_from(src: &[u8]) -> Result<IndexingProviderStateHash, Self::Error> {
        if src.len() != INDEXING_PROVIDER_STATE_HASH_LEN {
            Err(())
        } else {
            let mut inner = [0; INDEXING_PROVIDER_STATE_HASH_LEN];
            inner.clone_from_slice(src);
            Ok(IndexingProviderStateHash(inner))
        }
    }
}

impl AsRef<[u8]> for IndexingProviderStateHash {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}