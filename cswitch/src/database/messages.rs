use utils::crypto::identity::PublicKey;

use database::types::*;
use funder::types::InvoiceId;

pub enum DatabaseToNetworker {
    ResponseLoadNeighbors(Vec<NeighborConfig>),
    ResponseLoadNeighborToken {
        neighbor_public_key:     PublicKey,
        move_token_message_type: MoveTokenMessageType,
        move_token_message:      NeighborMoveTokenMessage,
        remote_max_debt:         u64,
        local_max_debt:          u64,
        remote_pending_debt:     u64,
        local_pending_debt:      u64,
        balance:                 u64,
        local_invoice_id:        Option<InvoiceId>,
        remote_invoice_id:       Option<InvoiceId>,
        pending_local_requests:  Vec<PendingLocalNeighborRequest>,
        pending_remote_requests: Vec<PendingLocalNeighborRequest>,
    }
}

pub enum DatabaseToFunder {
    ResponseLoadFriends(Vec<FriendConfig>),
    ResponseLoadFriendToken {
        friend_public_key:       PublicKey,
        move_token_message_type: MoveTokenMessageType,
        move_token_message:      FriendMoveTokenMessage,
        remote_max_debt:         u64,
        local_max_debt:          u64,
        remote_pending_debt:     u64,
        local_pending_debt:      u64,
        balance:                 u64,
        local_state:             u8,
        remote_state:            u8,
        pending_local_requests:  Vec<PendingLocalFriendRequest>,
        pending_remote_requests: Vec<PendingLocalFriendRequest>,
    }
}

pub enum DatabaseToIndexerClient {
    ResponseLoadIndexingProviders(Vec<IndexingProviderConfig>)
}
