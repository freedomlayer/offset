//! The messages sent by database to other components.

use crypto::identity::PublicKey;

use networker::messages::{MoveTokenDirection, NeighborInfo, 
    PendingNeighborRequest, NeighborTokenCommon};

use proto::funder::FriendMoveToken;
use funder::messages::{PendingFriendRequest, FriendInfo, 
    FriendRequestsStatus, FriendTokenCommon};

use indexer_client::messages::IndexingProviderStatus;
use proto::indexer::{IndexingProviderId, NeighborsRoute, StateChainLink};

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
    move_token_direction: MoveTokenDirection,
    friend_token_common: FriendTokenCommon,
    pending_local_requests: Vec<PendingFriendRequest>,
    pending_remote_requests: Vec<PendingFriendRequest>,
}

pub struct ResponseLoadFriends {
    pub friends: Vec<FriendInfo>,
}


pub struct ResponseLoadIndexingProviders(pub Vec<IndexingProviderInfoFromDB>);

pub struct ResponseLoadNeighbors {
    pub neighbors: Vec<NeighborInfo>,
}

pub struct ResponseLoadNeighborToken {
    move_token_direction: MoveTokenDirection,
    neighbor_token_common: NeighborTokenCommon,
    pending_local_requests: Vec<PendingNeighborRequest>,
    pending_remote_requests: Vec<PendingNeighborRequest>,
}

