
use super::token_channel::ProcessMessageError;
use proto::indexer::NeighborsRoute;
use crypto::hash::HashResult;
use crypto::uid::Uid;
use crypto::identity::PublicKey;
use super::messenger_messages::ResponseSendMessage;
use super::messenger_messages::FailedSendMessage;

#[derive(Clone)]
pub struct PendingNeighborRequest {
    pub request_id: Uid,
    pub route: NeighborsRoute,
    pub request_content_hash: HashResult,
    pub request_content_len: u32,
    pub max_response_len: u32,
    pub processing_fee_proposal: u64,
}

