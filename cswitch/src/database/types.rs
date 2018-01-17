use std::net::SocketAddr;

use crypto::identity::PublicKey;
use crypto::rand_values::RandValue;
use crypto::uuid::Uuid;
use crypto::hash::HashResult;

use funder::types::{InvoiceId, FunderTokenChannelTransaction};
use indexer::messages::RoutesToIndexer;
//use networker::types::NetworkerTokenChannelTransaction;
use indexer::types::{FriendsRouteWithCapacity, IndexingProviderId, NeighborsRoute, StateChainLink};

pub enum NeighborStatus {
    Enable = 1,
    Disable = 0,
}

/// The universal unique identifier used to identify a request.
pub type RequestId = Uuid;

/// The neighbor's information from database.
pub struct NeighborInfoFromDB {
    pub neighbor_public_key:    PublicKey,
    pub neighbor_socket_addr: Option<SocketAddr>,
    pub wanted_remote_max_debt: u64,
    pub wanted_max_channels:    u32,
    pub status:                 u8,
}

/// The friend's information from database.
pub struct FriendInfoFromDB {
    pub friend_public_key:      PublicKey,
    pub wanted_remote_max_debt: u128,
    pub status:                 u8,
}

/// The indexing provider's information from database.
pub struct IndexingProviderInfoFromDB {
    pub indexing_provider_id: IndexingProviderId,
    pub last_routes:          RoutesToIndexer,
    pub chain_link:           StateChainLink,
    pub status:               u8,
}

pub struct PendingNeighborRequest {
    pub request_id:                RequestId,
    pub route:                     NeighborsRoute,
    pub request_type: NeighborRequestType,
    pub request_content_hash:      HashResult,
    pub max_response_length:       u32,
    pub processing_fee_proposal:   u64,
    pub credits_per_byte_proposal: u32,
}

pub struct PendingFriendRequest {
    pub request_id:                RequestId,
    pub route:                     FriendsRouteWithCapacity,
    pub mediator_payment_proposal: u64,
    pub invoice_id:                InvoiceId,
    pub destination_payment:       u128,
}
