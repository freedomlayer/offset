#![warn(unused)]

use byteorder::{BigEndian, WriteBytesExt};
use crypto::hash;
use crypto::identity::verify_signature;
use super::types::{ResponseSendMessage, FailureSendMessage, PendingFriendRequest};

const REQUEST_SUCCESS_PREFIX: &[u8] = b"REQUEST_SUCCESS";
const REQUEST_FAILURE_PREFIX: &[u8] = b"REQUEST_FAILURE";

/// Create the buffer we sign over at the Response message.
/// Note that the signature is not just over the Response message bytes. The signed buffer also
/// contains information from the Request message.
pub fn create_response_signature_buffer(response_send_msg: &ResponseSendMessage,
                        pending_request: &PendingFriendRequest) -> Vec<u8> {

    let mut sbuffer = Vec::new();

    // TODO: Add a const for this:
    sbuffer.extend_from_slice(&hash::sha_512_256(REQUEST_SUCCESS_PREFIX));
    sbuffer.extend_from_slice(&pending_request.request_id);
    sbuffer.write_u32::<BigEndian>(pending_request.max_response_len).expect("Serialization failure!");
    sbuffer.write_u64::<BigEndian>(pending_request.processing_fee_proposal).expect("Serialization failure!");
    sbuffer.extend_from_slice(&pending_request.route.hash());
    sbuffer.extend_from_slice(&pending_request.request_content_hash);
    sbuffer.write_u64::<BigEndian>(response_send_msg.processing_fee_collected).expect("Serialization failure!");
    sbuffer.extend_from_slice(&hash::sha_512_256(&response_send_msg.response_content));
    sbuffer.extend_from_slice(&response_send_msg.rand_nonce);

    sbuffer
}

/// Create the buffer we sign over at the Failure message.
/// Note that the signature is not just over the Response message bytes. The signed buffer also
/// contains information from the Request message.
pub fn create_failure_signature_buffer(failure_send_msg: &FailureSendMessage,
                        pending_request: &PendingFriendRequest) -> Vec<u8> {

    let mut sbuffer = Vec::new();

    // TODO: Add a const for this:
    sbuffer.extend_from_slice(&hash::sha_512_256(REQUEST_FAILURE_PREFIX));
    sbuffer.extend_from_slice(&pending_request.request_id);
    sbuffer.write_u32::<BigEndian>(pending_request.max_response_len).expect("Serialization failure!");
    sbuffer.write_u64::<BigEndian>(pending_request.processing_fee_proposal).expect("Serialization failure!");
    sbuffer.extend_from_slice(&pending_request.route.hash());
    sbuffer.extend_from_slice(&pending_request.request_content_hash);
    sbuffer.extend_from_slice(&failure_send_msg.reporting_public_key);

    sbuffer
}

// TODO: How to test this?


/// Verify all failure message signature chain
pub fn verify_failure_signature(index: usize,
                            reporting_index: usize,
                            failure_send_msg: &FailureSendMessage,
                            pending_request: &PendingFriendRequest) -> Option<()> {

    let mut failure_signature_buffer = create_failure_signature_buffer(
                                        &failure_send_msg,
                                        &pending_request);
    let next_index = index.checked_add(1)?;
    for i in (next_index ..= reporting_index).rev() {
        let sig_index = i.checked_sub(next_index)?;
        let rand_nonce = &failure_send_msg.rand_nonce_signatures[sig_index].rand_nonce;
        let signature = &failure_send_msg.rand_nonce_signatures[sig_index].signature;
        failure_signature_buffer.extend_from_slice(rand_nonce);
        let public_key = pending_request.route.pk_by_index(i)?;
        if !verify_signature(&failure_signature_buffer, public_key, signature) {
            return None;
        }
    }
    Some(())
}

