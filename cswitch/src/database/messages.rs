use crypto::identity::PublicKey;

use database::types::*;
use networker::types::NeighborMoveToken;
use funder::types::{FriendMoveToken, InvoiceId};

pub enum DatabaseToNetworker {
    ResponseLoadNeighbors {
        neighbors: Vec<NeighborInfoFromDB>
    },
    ResponseLoadNeighborToken {
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
    },
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

pub enum DatabaseToIndexerClient {
    ResponseLoadIndexingProviders(Vec<IndexingProviderInfoFromDB>)
}
