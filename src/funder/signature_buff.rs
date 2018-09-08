#![warn(unused)]

use byteorder::{BigEndian, WriteBytesExt};
use crypto::hash;
use crypto::identity::{verify_signature, PublicKey};
use super::types::{ResponseSendFunds, FailureSendFunds, 
    PendingFriendRequest, SendFundsReceipt};

pub const FUND_SUCCESS_PREFIX: &[u8] = b"FUND_SUCCESS";
pub const FUND_FAILURE_PREFIX: &[u8] = b"FUND_FAILURE";

/// Create the buffer we sign over at the Response funds.
/// Note that the signature is not just over the Response funds bytes. The signed buffer also
/// contains information from the Request funds.
pub fn create_response_signature_buffer(response_send_funds: &ResponseSendFunds,
                        pending_request: &PendingFriendRequest) -> Vec<u8> {

    let mut sbuffer = Vec::new();

    sbuffer.extend_from_slice(&hash::sha_512_256(FUND_SUCCESS_PREFIX));

    let mut inner_blob = Vec::new();
    inner_blob.extend_from_slice(&pending_request.request_id);
    inner_blob.extend_from_slice(&pending_request.route.hash());
    inner_blob.extend_from_slice(&response_send_funds.rand_nonce);

    sbuffer.extend_from_slice(&hash::sha_512_256(&inner_blob));
    sbuffer.write_u128::<BigEndian>(pending_request.dest_payment).unwrap();
    sbuffer.extend_from_slice(&pending_request.invoice_id);

    sbuffer
}

/// Create the buffer we sign over at the Failure funds.
/// Note that the signature is not just over the Response funds bytes. The signed buffer also
/// contains information from the Request funds.
pub fn create_failure_signature_buffer(failure_send_funds: &FailureSendFunds,
                        pending_request: &PendingFriendRequest) -> Vec<u8> {

    let mut sbuffer = Vec::new();

    sbuffer.extend_from_slice(&hash::sha_512_256(FUND_FAILURE_PREFIX));
    sbuffer.extend_from_slice(&pending_request.request_id);
    sbuffer.extend_from_slice(&pending_request.route.hash());

    sbuffer.write_u128::<BigEndian>(pending_request.dest_payment).unwrap();
    sbuffer.extend_from_slice(&pending_request.invoice_id);
    sbuffer.extend_from_slice(&failure_send_funds.reporting_public_key);
    sbuffer.extend_from_slice(&failure_send_funds.rand_nonce);

    sbuffer
}

/// Verify a failure signature
pub fn verify_failure_signature(failure_send_funds: &FailureSendFunds,
                            pending_request: &PendingFriendRequest) -> Option<()> {

    let failure_signature_buffer = create_failure_signature_buffer(
                                        &failure_send_funds,
                                        &pending_request);
    let reporting_public_key = &failure_send_funds.reporting_public_key;
    // Make sure that the reporting_public_key is on the route:
    // TODO: Should we check that it is after us? Is it checked somewhere else?
    let _ = pending_request.route.pk_to_index(&reporting_public_key)?;

    if !verify_signature(&failure_signature_buffer, 
                     reporting_public_key, 
                     &failure_send_funds.signature) {
        return None;
    }
    Some(())
}

pub fn prepare_receipt(response_send_funds: &ResponseSendFunds,
                    pending_request: &PendingFriendRequest) -> SendFundsReceipt {

    let mut hash_buff = Vec::new();
    hash_buff.extend_from_slice(&pending_request.request_id);
    hash_buff.extend_from_slice(&pending_request.route.to_bytes());
    hash_buff.extend_from_slice(&response_send_funds.rand_nonce);
    let response_hash = hash::sha_512_256(&hash_buff);
    // = sha512/256(requestId || sha512/256(route) || randNonce)

    SendFundsReceipt {
        response_hash,
        invoice_id: pending_request.invoice_id.clone(),
        dest_payment: pending_request.dest_payment,
        signature: response_send_funds.signature.clone(),
    }
}


#[allow(unused)]
pub fn verify_receipt(receipt: &SendFundsReceipt,
                      public_key: &PublicKey) -> bool {
    let mut data = Vec::new();
    data.extend(FUND_SUCCESS_PREFIX);
    data.extend(receipt.response_hash.as_ref());
    data.extend(receipt.invoice_id.as_ref());
    data.write_u128::<BigEndian>(receipt.dest_payment).unwrap();
    verify_signature(&data, public_key, &receipt.signature)
}


// TODO: How to test this?
