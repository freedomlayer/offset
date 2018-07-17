//! The messages sent by database to other components.

use networker::messages::{NeighborInfo, NeighborTokenCommon};
use networker::messenger::types::PendingNeighborRequest;
use funder::messages::{PendingFriendRequest, FriendInfo, FriendTokenCommon};


/*
/// The friend's information from database.
pub struct FriendInfo {
    pub friend_public_key: PublicKey,
    pub wanted_remote_max_debt: u128,
    pub status: u8,
}
*/

pub struct ResponseLoadFriendToken {
    // pub move_token_direction: MoveTokenDirection,
    pub friend_token_common: FriendTokenCommon,
    pub pending_local_requests: Vec<PendingFriendRequest>,
    pub pending_remote_requests: Vec<PendingFriendRequest>,
}

pub struct ResponseLoadFriends {
    pub friends: Vec<FriendInfo>,
}



pub struct ResponseLoadNeighbors {
    pub neighbors: Vec<NeighborInfo>,
}

pub struct ResponseLoadNeighborToken {
    // pub move_token_direction: MoveTokenDirection,
    pub neighbor_token_common: NeighborTokenCommon,
    pub pending_local_requests: Vec<PendingNeighborRequest>,
    pub pending_remote_requests: Vec<PendingNeighborRequest>,
}

