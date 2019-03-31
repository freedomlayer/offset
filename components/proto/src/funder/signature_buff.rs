use byteorder::{BigEndian, WriteBytesExt};
use crypto::hash::{self, sha_512_256, HashResult};
use crypto::identity::{verify_signature, PublicKey};

use common::canonical_serialize::CanonicalSerialize;
use common::int_convert::usize_to_u64;

use super::messages::{FailureSendFunds, MoveToken, PendingRequest, Receipt, ResponseSendFunds};

pub const FUND_SUCCESS_PREFIX: &[u8] = b"FUND_SUCCESS";
pub const FUND_FAILURE_PREFIX: &[u8] = b"FUND_FAILURE";

/// Create the buffer we sign over at the Response funds.
/// Note that the signature is not just over the Response funds bytes. The signed buffer also
/// contains information from the Request funds.
pub fn create_response_signature_buffer<S>(
    response_send_funds: &ResponseSendFunds<S>,
    pending_request: &PendingRequest,
) -> Vec<u8> {
    let mut sbuffer = Vec::new();

    sbuffer.extend_from_slice(&hash::sha_512_256(FUND_SUCCESS_PREFIX));

    let mut inner_blob = Vec::new();
    inner_blob.extend_from_slice(&pending_request.request_id);
    inner_blob.extend_from_slice(&pending_request.route.hash());
    inner_blob.extend_from_slice(&response_send_funds.rand_nonce);

    sbuffer.extend_from_slice(&hash::sha_512_256(&inner_blob));
    sbuffer
        .write_u128::<BigEndian>(pending_request.dest_payment)
        .unwrap();
    sbuffer.extend_from_slice(&pending_request.invoice_id);

    sbuffer
}

/// Create the buffer we sign over at the Failure funds.
/// Note that the signature is not just over the Response funds bytes. The signed buffer also
/// contains information from the Request funds.
pub fn create_failure_signature_buffer<S>(
    failure_send_funds: &FailureSendFunds<S>,
    pending_request: &PendingRequest,
) -> Vec<u8> {
    let mut sbuffer = Vec::new();

    sbuffer.extend_from_slice(&hash::sha_512_256(FUND_FAILURE_PREFIX));
    sbuffer.extend_from_slice(&pending_request.request_id);
    sbuffer.extend_from_slice(&pending_request.route.hash());

    sbuffer
        .write_u128::<BigEndian>(pending_request.dest_payment)
        .unwrap();
    sbuffer.extend_from_slice(&pending_request.invoice_id);
    sbuffer.extend_from_slice(&failure_send_funds.reporting_public_key);
    sbuffer.extend_from_slice(&failure_send_funds.rand_nonce);

    sbuffer
}

/// Verify a failure signature
pub fn verify_failure_signature(
    failure_send_funds: &FailureSendFunds,
    pending_request: &PendingRequest,
) -> Option<()> {
    let failure_signature_buffer =
        create_failure_signature_buffer(&failure_send_funds, &pending_request);
    let reporting_public_key = &failure_send_funds.reporting_public_key;
    // Make sure that the reporting_public_key is on the route:
    // TODO: Should we check that it is after us? Is it checked somewhere else?
    let _ = pending_request.route.pk_to_index(&reporting_public_key)?;

    if !verify_signature(
        &failure_signature_buffer,
        reporting_public_key,
        &failure_send_funds.signature,
    ) {
        return None;
    }
    Some(())
}

pub fn prepare_receipt(
    response_send_funds: &ResponseSendFunds,
    pending_request: &PendingRequest,
) -> Receipt {
    let mut hash_buff = Vec::new();
    hash_buff.extend_from_slice(&pending_request.request_id);
    hash_buff.extend_from_slice(&pending_request.route.canonical_serialize());
    hash_buff.extend_from_slice(&response_send_funds.rand_nonce);
    let response_hash = hash::sha_512_256(&hash_buff);
    // = sha512/256(requestId || sha512/256(route) || randNonce)

    Receipt {
        response_hash,
        invoice_id: pending_request.invoice_id.clone(),
        dest_payment: pending_request.dest_payment,
        signature: response_send_funds.signature.clone(),
    }
}

#[allow(unused)]
pub fn verify_receipt(receipt: &Receipt, public_key: &PublicKey) -> bool {
    let mut data = Vec::new();
    data.extend(FUND_SUCCESS_PREFIX);
    data.extend(receipt.response_hash.as_ref());
    data.extend(receipt.invoice_id.as_ref());
    data.write_u128::<BigEndian>(receipt.dest_payment).unwrap();
    verify_signature(&data, public_key, &receipt.signature)
}

// Prefix used for chain hashing of token channel funds.
// NEXT is used for hashing for the next move token funds.
const TOKEN_NEXT: &[u8] = b"NEXT";

/// Combine all operations into one hash value.
pub fn operations_hash<B>(move_token: &MoveToken<B>) -> HashResult {
    let mut operations_data = Vec::new();
    operations_data
        .write_u64::<BigEndian>(usize_to_u64(move_token.operations.len()).unwrap())
        .unwrap();
    for op in &move_token.operations {
        operations_data.extend_from_slice(&op.canonical_serialize());
    }
    sha_512_256(&operations_data)
}

/// Combine all operations into one hash value.
pub fn local_address_hash<B>(move_token: &MoveToken<B>) -> HashResult
where
    B: CanonicalSerialize,
{
    sha_512_256(&move_token.opt_local_relays.canonical_serialize())
}

/// Hash operations and local_address:
pub fn prefix_hash<B, S>(move_token: &MoveToken<B, S>) -> HashResult
where
    B: CanonicalSerialize,
{
    let mut hash_buff = Vec::new();

    hash_buff.extend_from_slice(&move_token.old_token);

    // TODO; Use CanonicalSerialize instead here:
    hash_buff
        .write_u64::<BigEndian>(usize_to_u64(move_token.operations.len()).unwrap())
        .unwrap();
    for op in &move_token.operations {
        hash_buff.extend_from_slice(&op.canonical_serialize());
    }

    hash_buff.extend_from_slice(&move_token.opt_local_relays.canonical_serialize());
    sha_512_256(&hash_buff)
}

pub fn move_token_signature_buff<B, S>(move_token: &MoveToken<B, S>) -> Vec<u8>
where
    B: CanonicalSerialize,
{
    let mut sig_buffer = Vec::new();
    sig_buffer.extend_from_slice(&sha_512_256(TOKEN_NEXT));
    sig_buffer.extend_from_slice(&prefix_hash(move_token));
    sig_buffer
        .write_u64::<BigEndian>(move_token.inconsistency_counter)
        .unwrap();
    sig_buffer
        .write_u128::<BigEndian>(move_token.move_token_counter)
        .unwrap();
    sig_buffer
        .write_i128::<BigEndian>(move_token.balance)
        .unwrap();
    sig_buffer
        .write_u128::<BigEndian>(move_token.local_pending_debt)
        .unwrap();
    sig_buffer
        .write_u128::<BigEndian>(move_token.remote_pending_debt)
        .unwrap();
    sig_buffer.extend_from_slice(&move_token.rand_nonce);

    sig_buffer
}

/// Verify that new_token is a valid signature over the rest of the fields.
pub fn verify_move_token<B>(move_token: &MoveToken<B>, public_key: &PublicKey) -> bool
where
    B: CanonicalSerialize,
{
    let sig_buffer = move_token_signature_buff(move_token);
    verify_signature(&sig_buffer, public_key, &move_token.new_token)
}

// TODO: How to test this?
