use crypto::hash::HashResult;
use crypto::uid::Uid;
use crypto::identity::PublicKey;

use super::token_channel::ProcessMessageError;
use super::types::{ResponseSendMessage, FailureSendMessage, NeighborsRoute};

#[derive(Clone)]
pub struct PendingNeighborRequest {
    pub request_id: Uid,
    pub route: NeighborsRoute,
    pub request_content_hash: HashResult,
    pub request_content_len: u32,
    pub max_response_len: u32,
    pub processing_fee_proposal: u64,
}
