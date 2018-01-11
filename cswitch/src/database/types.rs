// TODO: Move inner_messages's type into its module.
use inner_messages::{IndexingProviderId, NeighborsRoute, FriendsRoute,
                     StateChainLink};

use networker::types::TokenChannelTransaction;

use schema::indexer::IndexerRoute;

pub struct NeighborConfig {
    neighbor_public_key: PublicKey,
    remote_maximum_debt: u64,
    maximum_channel:     u32,
    is_enable:           bool,
}


pub struct PendingLocalNeighborRequest {
    request_id:              Uid,
    route:                   NeighborsRoute,
    request_content_hash:    [u8; 32],
    maximum_response_length: u32,
    processing_fee_proposal: u64,
    half_credits_per_byte_proposal: u32,
}

pub struct MoveTokenMessage {
    transactions: Vec<TokenChannelTransaction>,
    old_token:    Bytes,
    rand_nonce:   RandValue,
}

pub struct FriendConfig {
    friend_public_key:   PublicKey,
    remote_maximum_debt: u128,
    is_enable:           bool,
}

pub struct PendingLocalFriendRequest {
    request_id:                Uid,
    route:                     FriendsRoute,
    mediator_payment_proposal: u64,
    invoice_id:                InvoiceId,
    destination_payment:       i128,
}

pub struct IndexingProviderConfig {
    indexing_provider_id: IndexingProviderId,
    last_route:           IndexerRoute,
    chain_link:           StateChainLink,
    is_enable:            bool,
}
