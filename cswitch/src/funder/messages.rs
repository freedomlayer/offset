use crypto::identity::PublicKey;
use crypto::uuid::Uuid;

use proto::indexer::FriendsRouteWithCapacity;
use networker::messages::MoveTokenDirection;
use proto::funder::{InvoiceId, FriendMoveToken};

pub enum FriendStatus {
    Enable = 1,
    Disable = 0,
}

/// The friend's information from database.
pub struct FriendInfoFromDB {
    pub friend_public_key:      PublicKey,
    pub wanted_remote_max_debt: u128,
    pub status:                 u8,
}

pub struct PendingFriendRequest {
    pub request_id:                Uuid,
    pub route:                     FriendsRouteWithCapacity,
    pub mediator_payment_proposal: u64,
    pub invoice_id:                InvoiceId,
    pub destination_payment:       u128,
}

// ========== Interfaces with Database ==========

// TODO:
pub enum FunderToDatabase {
    StoreFriend {
        friend_public_key: PublicKey,
        wanted_remote_max_debt: u128,
        status: FriendStatus,
    },
    RemoveFriend {},
    RequestLoadFriends {},
    StoreInFriendToken {},
    StoreOutFriendToken {},
    RequestLoadFriendToken {},
}

pub enum DatabaseToFunder {
    ResponseLoadFriends {
        friends: Vec<FriendInfoFromDB>
    },
    ResponseLoadFriendToken {
        friend_public_key: PublicKey,
        move_token_direction: MoveTokenDirection,
        move_token_message: FriendMoveToken,
        remote_max_debt: u64,
        local_max_debt: u64,
        remote_pending_debt: u64,
        local_pending_debt: u64,
        balance: i64,
        local_state: u8,
        remote_state: u8,
        pending_local_requests: Vec<PendingFriendRequest>,
        pending_remote_requests: Vec<PendingFriendRequest>,
    },
}