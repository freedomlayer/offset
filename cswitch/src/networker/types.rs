use bytes::Bytes;

use utils::crypto::dh::{DhPublicKey,Salt};
use utils::crypto::identity::Signature;

// TODO:
pub struct RequestContent {}

pub struct EncryptedContent {
    sender_ephemeral_public_key: DhPublicKey,
    key_derivation_nonce: Salt,
    destination_comm_public_key_hash: [u8; 8],
    signature: Signature,
    content: Bytes,
}

// TODO:
pub struct ResponseContent {}

pub struct NeighborMoveTokenMessage {
    pub channel_index: u32,
    pub transactions:  Vec<NetworkerTokenChannelTransaction>,
    pub old_token:     Bytes,
    pub rand_nonce:    RandValue,
}

pub enum NetworkerTokenChannelTransaction {
    SetRemoteMaximumDebt(u64),
    FundsRandNonce(RandValue),
    // TODO: Find out read structure
    LoadFunds {
        funds_rand_nonce: RandValue,
        // payment_id: ?,
        // num_credits: ?,
        // signature: ?
    },
    RequestSendMessage {
        request_id: Uid,
        route: NeighborRoute,
        request_content: RequestContent,
        maximum_response_length: u32,
        processing_fee_proposal: u64,
        half_credits_per_byte_proposal: u32,
    },
    ResponseSendMessage {
        request_id: Uuid,
        response_content: ResponseContent,
        signature: Signature,
    },
    FailedSendMessage {
        request_id: Uuid,
        reporting_node_public_key: PublicKey,
        signature: Signature,
    },
    ResetChannel {
        new_balance: i128,
    },
}
