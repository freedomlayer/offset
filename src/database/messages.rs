//! The messages sent by database to other components.

use crypto::identity::PublicKey;

use networker::messages::{MoveTokenDirection, NeighborInfo, PendingNeighborRequest};

use proto::funder::{FriendMoveToken, InvoiceId};
use funder::messages::{PendingFriendRequest, FriendInfo, FriendRequestsStatus};

use indexer_client::messages::IndexingProviderStatus;
use proto::indexer::{IndexingProviderId, NeighborsRoute, StateChainLink};
use proto::networker::NeighborMoveToken;

/// The indexing provider's information from database.
pub struct IndexingProviderInfoFromDB {
    id: IndexingProviderId,
    state_chain_link: StateChainLink,
    last_routes: Vec<NeighborsRoute>,
    status: IndexingProviderStatus,
}

/*
/// The friend's information from database.
pub struct FriendInfo {
    pub friend_public_key: PublicKey,
    pub wanted_remote_max_debt: u128,
    pub status: u8,
}
*/

pub struct ResponseLoadFriendToken {
    friend_public_key: PublicKey,
    move_token_direction: MoveTokenDirection,
    move_token_message: FriendMoveToken,
    remote_max_debt: u64,
    local_max_debt: u64,
    remote_pending_debt: u64,
    local_pending_debt: u64,
    balance: i64,
    local_state: FriendRequestsStatus,
    remote_state: FriendRequestsStatus,
    pending_local_requests: Vec<PendingFriendRequest>,
    pending_remote_requests: Vec<PendingFriendRequest>,
}

pub struct ResponseLoadFriends {
    friends: Vec<FriendInfo>,
}


pub enum DatabaseToIndexerClient {
    ResponseLoadIndexingProviders(Vec<IndexingProviderInfoFromDB>),
}

pub struct ResponseLoadNeighbors {
    neighbors: Vec<NeighborInfo>,
}

pub struct ResponseLoadNeighborToken {
    neighbor_public_key: PublicKey,
    move_token_direction: MoveTokenDirection,
    move_token_message: NeighborMoveToken,
    remote_max_debt: u64,
    local_max_debt: u64,
    remote_pending_debt: u64,
    local_pending_debt: u64,
    balance: i64,
    local_invoice_id: Option<InvoiceId>,
    remote_invoice_id: Option<InvoiceId>,
    pending_local_requests: Vec<PendingNeighborRequest>,
    pending_remote_requests: Vec<PendingNeighborRequest>,
}

