use bytes::Bytes;

use utils::crypto::identity::PublicKey;
use utils::crypto::rand_values::RandValue;
use utils::crypto::uuid::Uuid;

use funder::types::{InvoiceId, FunderTokenChannelTransaction};
use indexer::messages::RoutesToIndexer;
use networker::types::NetworkerTokenChannelTransaction;
use indexer::types::{FriendsRouteWithCapacity, IndexingProviderId, NeighborsRoute, StateChainLink};

/// The request type.
#[derive(Clone, Copy, Debug)]
pub enum RequestType {
    CommMeans = 0,
    Encrypted = 1,
}

/// Request direction.
#[derive(Clone, Copy, Debug)]
pub enum RequestDirection {
    /// Indicate the request is an incoming request.
    Incoming = 0,
    /// Indicate the request is an outgoing request.
    Outgoing = 1,
}

/// The universal unique identifier used to identify a request.
pub type RequestId = Uuid;

/// The neighbor's information from database.
pub struct NeighborConfig {
    pub neighbor_public_key:    PublicKey,
    pub wanted_remote_max_debt: u64,
    pub wanted_max_channels:    u32,
    pub status:                 u8,
}

/// The friend's information from database.
pub struct FriendConfig {
    pub friend_public_key:      PublicKey,
    pub wanted_remote_max_debt: u128,
    pub status:                 u8,
}

/// The indexing provider's information from database.
pub struct IndexingProviderConfig {
    pub indexing_provider_id: IndexingProviderId,
    pub last_routes:          RoutesToIndexer,
    pub chain_link:           StateChainLink,
    pub status:               u8,
}

pub struct PendingNeighborRequest {
    pub request_id:                RequestId,
    pub route:                     NeighborsRoute,
    pub request_type:              RequestType,
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
