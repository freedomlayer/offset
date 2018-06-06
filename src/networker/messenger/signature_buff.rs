use byteorder::{BigEndian, WriteBytesExt};

use crypto::hash;

use super::messenger_messages::{ResponseSendMessage, FailureSendMessage};
use super::pending_neighbor_request::PendingNeighborRequest;

/// Create the buffer we sign over at the Response message.
/// Note that the signature is not just over the Response message bytes. The signed buffer also
/// contains information from the Request message.
pub fn create_response_signature_buffer(response_send_msg: &ResponseSendMessage,
                        pending_request: &PendingNeighborRequest) -> Vec<u8> {

    let mut sbuffer = Vec::new();

    // TODO: Add a const for this:
    sbuffer.extend_from_slice(&hash::sha_512_256(b"REQUEST_SUCCESS"));
    sbuffer.extend_from_slice(&pending_request.request_id);
    sbuffer.write_u32::<BigEndian>(pending_request.max_response_len);
    sbuffer.write_u64::<BigEndian>(pending_request.processing_fee_proposal);
    sbuffer.extend_from_slice(&pending_request.route.hash());
    sbuffer.extend_from_slice(&pending_request.request_content_hash);
    sbuffer.write_u64::<BigEndian>(response_send_msg.processing_fee_collected);
    sbuffer.extend_from_slice(&hash::sha_512_256(&response_send_msg.response_content));
    sbuffer.extend_from_slice(&response_send_msg.rand_nonce);

    sbuffer
}

/// Create the buffer we sign over at the Response message.
/// Note that the signature is not just over the Response message bytes. The signed buffer also
/// contains information from the Request message.
pub fn create_failure_signature_buffer(failure_send_msg: &FailureSendMessage,
                        pending_request: &PendingNeighborRequest) -> Vec<u8> {

    let mut sbuffer = Vec::new();

    // TODO: Add a const for this:
    sbuffer.extend_from_slice(&hash::sha_512_256(b"REQUEST_FAILURE"));
    sbuffer.extend_from_slice(&pending_request.request_id);
    sbuffer.write_u32::<BigEndian>(pending_request.max_response_len);
    sbuffer.write_u64::<BigEndian>(pending_request.processing_fee_proposal);
    sbuffer.extend_from_slice(&pending_request.route.hash());
    sbuffer.extend_from_slice(&pending_request.request_content_hash);
    sbuffer.extend_from_slice(&failure_send_msg.reporting_public_key);

    sbuffer
}

// TODO: How to test this?
