//! The internal messages send from Database Module to other components.

use crypto::identity::PublicKey;
use crypto::rand_values::RandValue;

pub enum DatabaseToNetworker {
    ResponseLoadNeighbors(Vec<NeighborConfig>),
    ResponseLoadNeighborToken {
        neighbor_public_key:     PublicKey,
        move_token_message_type: u8,
        move_token_message:      NeighborMoveTokenMessage,
        remote_maximum_debt:     u64,
        local_maximum_debt:      u64,
        remote_pending_debt:     u64,
        local_pending_debt:      u64,
        balance:                 u64,
        local_funds_rand_nonce:  Option<RandValue>,
        remote_funds_rand_nonce: Option<RandValue>,
    }
}

pub enum DatabaseToFunder {
    ResponseLoadFriends(Vec<FriendConfig>),
    ResponseLoadFriendToken {
        friend_public_key:       PublicKey,
        move_token_message_type: u8,
        move_token_message:      FriendMoveTokenMessage,
        remote_maximum_debt:     u64,
        local_maximum_debt:      u64,
        remote_pending_debt:     u64,
        local_pending_debt:      u64,
        balance:                 u64,
        is_local_enable:         bool,
        is_remote_enable:        bool,
        pending_local_requests:  Vec<PendingLocalFriendRequest>,
    }
}

pub enum DatabaseToIndexerClient {
    ResponseLoadIndexingProviders(Vec<IndexingProviderConfig>)
}