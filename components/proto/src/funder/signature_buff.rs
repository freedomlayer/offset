use byteorder::{BigEndian, WriteBytesExt};
use crypto::hash::{self, sha_512_256, HashResult};

use common::int_convert::usize_to_u64;
use common::canonical_serialize::CanonicalSerialize;

use crate::funder::messages::{ResponseSendFunds, FailureSendFunds, 
    SendFundsReceipt, PendingRequest, MoveToken,
    SignedResponse};

pub const FUND_SUCCESS_PREFIX: &[u8] = b"FUND_SUCCESS";
pub const FUND_FAILURE_PREFIX: &[u8] = b"FUND_FAILURE";

/// Create the buffer we sign over at the Response funds.
/// Note that the signature is not just over the Response funds bytes. The signed buffer also
/// contains information from the Request funds.
pub fn create_response_signature_buffer<P,PRS>(response_send_funds: &ResponseSendFunds<PRS>,
                        pending_request: &PendingRequest<P>) -> Vec<u8> 
where
    P: CanonicalSerialize + Eq + std::hash::Hash,
{

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
pub fn create_failure_signature_buffer<P,PFS>(failure_send_funds: &FailureSendFunds<P,PFS>,
                        pending_request: &PendingRequest<P>) -> Vec<u8> 
where
    P: CanonicalSerialize + Eq + std::hash::Hash,
{

    let mut sbuffer = Vec::new();

    sbuffer.extend_from_slice(&hash::sha_512_256(FUND_FAILURE_PREFIX));
    sbuffer.extend_from_slice(&pending_request.request_id);
    sbuffer.extend_from_slice(&pending_request.route.hash());

    sbuffer.write_u128::<BigEndian>(pending_request.dest_payment).unwrap();
    sbuffer.extend_from_slice(&pending_request.invoice_id);
    sbuffer.extend_from_slice(&failure_send_funds.reporting_public_key.canonical_serialize());
    sbuffer.extend_from_slice(&failure_send_funds.rand_nonce);

    sbuffer
}


pub fn prepare_receipt<P,RS>(response_send_funds: &SignedResponse<RS>,
                    pending_request: &PendingRequest<P>) -> SendFundsReceipt<RS> 
where
    P: CanonicalSerialize,
    RS: Clone,
{

    let mut hash_buff = Vec::new();
    hash_buff.extend_from_slice(&pending_request.request_id);
    hash_buff.extend_from_slice(&pending_request.route.canonical_serialize());
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


// TODO: Possibly create a common code for this function and create_response_signature_buffer,
// because we have to keep them in sync.
pub fn create_receipt_signature_buffer<RS>(receipt: &SendFundsReceipt<RS>) -> Vec<u8> {
    let mut data = Vec::new();
    data.extend(FUND_SUCCESS_PREFIX);
    data.extend(receipt.response_hash.as_ref());
    data.extend(receipt.invoice_id.as_ref());
    data.write_u128::<BigEndian>(receipt.dest_payment).unwrap();
    data
}



// Prefix used for chain hashing of token channel funds.
// NEXT is used for hashing for the next move token funds.
const TOKEN_NEXT: &[u8] = b"NEXT";

/// Combine all operations into one hash value.
pub fn operations_hash<A,P,RS,FS,MS,PMS>(move_token: &MoveToken<A,P,RS,FS,MS,PMS>) -> HashResult 
where
    P: CanonicalSerialize,
    RS: CanonicalSerialize,
    FS: CanonicalSerialize,
{
    let mut operations_data = Vec::new();
    operations_data.write_u64::<BigEndian>(
        usize_to_u64(move_token.operations.len()).unwrap()).unwrap();
    for op in &move_token.operations {
        operations_data.extend_from_slice(&op.canonical_serialize());
    }
    sha_512_256(&operations_data)
}

/// Combine all operations into one hash value.
pub fn local_address_hash<A,P,RS,FS,MS,PMS>(move_token: &MoveToken<A,P,RS,FS,MS,PMS>) -> HashResult 
where
    A: CanonicalSerialize,
{
    sha_512_256(&move_token.opt_local_address.canonical_serialize())
}

/// Hash operations and local_address:
pub fn prefix_hash<A,P,RS,FS,MS,PMS>(move_token: &MoveToken<A,P,RS,FS,MS,PMS>) -> HashResult 
where
    A: CanonicalSerialize,
    P: CanonicalSerialize,
    RS: CanonicalSerialize,
    FS: CanonicalSerialize,
    MS: CanonicalSerialize,
{
    let mut hash_buff = Vec::new();

    hash_buff.extend_from_slice(&move_token.old_token.canonical_serialize());

    // TODO; Use CanonicalSerialize instead here:
    hash_buff.write_u64::<BigEndian>(
        usize_to_u64(move_token.operations.len()).unwrap()).unwrap();
    for op in &move_token.operations {
        hash_buff.extend_from_slice(&op.canonical_serialize());
    }

    hash_buff.extend_from_slice(&move_token.opt_local_address.canonical_serialize());
    sha_512_256(&hash_buff)
}

pub fn move_token_signature_buff<A,P,RS,FS,MS,PMS>(move_token: &MoveToken<A,P,RS,FS,MS,PMS>) -> Vec<u8> 
where
    A: CanonicalSerialize,
    P: CanonicalSerialize,
    RS: CanonicalSerialize,
    FS: CanonicalSerialize,
    MS: CanonicalSerialize,
{
    let mut sig_buffer = Vec::new();
    sig_buffer.extend_from_slice(&sha_512_256(TOKEN_NEXT));
    sig_buffer.extend_from_slice(&prefix_hash(move_token));
    sig_buffer.extend_from_slice(&move_token.local_public_key.canonical_serialize());
    sig_buffer.extend_from_slice(&move_token.remote_public_key.canonical_serialize());
    sig_buffer.write_u64::<BigEndian>(move_token.inconsistency_counter).unwrap();
    sig_buffer.write_u128::<BigEndian>(move_token.move_token_counter).unwrap();
    sig_buffer.write_i128::<BigEndian>(move_token.balance).unwrap();
    sig_buffer.write_u128::<BigEndian>(move_token.local_pending_debt).unwrap();
    sig_buffer.write_u128::<BigEndian>(move_token.remote_pending_debt).unwrap();
    sig_buffer.extend_from_slice(&move_token.rand_nonce);

    sig_buffer
}

