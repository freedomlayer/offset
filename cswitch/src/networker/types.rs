use bytes::Bytes;

// TODO:
pub struct RequestContent {}

pub struct EncryptedContent {
    sender_ephemeral_public_key: DhPublicKey,
    key_derivation_nonce: KeySalt,
    destination_comm_public_key_hash: [u8; 8],
    signature: Signature,
    content: Bytes,
}

// TODO:
pub struct ResponseContent {}

pub enum TokenChannelTransaction {
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
        request_id: Uid,
        response_content: ResponseContent,
        signature: Signature,
    },
    FailedSendMessage {
        request_id: Uid,
        reporting_node_public_key: PublicKey,
        signature: Signature,
    },
    ResetChannel {
        new_balance: i128,
    },
}
