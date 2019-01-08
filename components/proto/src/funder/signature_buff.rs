use byteorder::{BigEndian, WriteBytesExt};
use crypto::hash::{self, sha_512_256, HashResult};
use crypto::identity::{verify_signature, PublicKey, Signature};

use common::int_convert::usize_to_u64;
use common::canonical_serialize::CanonicalSerialize;
use crate::verify::Verify;

use crate::funder::messages::{ResponseSendFunds, FailureSendFunds, 
    SendFundsReceipt, PendingRequest, MoveToken, TPublicKey};

pub const FUND_SUCCESS_PREFIX: &[u8] = b"FUND_SUCCESS";
pub const FUND_FAILURE_PREFIX: &[u8] = b"FUND_FAILURE";

/// Create the buffer we sign over at the Response funds.
/// Note that the signature is not just over the Response funds bytes. The signed buffer also
/// contains information from the Request funds.
pub fn create_response_signature_buffer<P,RS>(response_send_funds: &ResponseSendFunds<RS>,
                        pending_request: &PendingRequest<P>) -> Vec<u8> 
where
    P: CanonicalSerialize + Eq + std::hash::Hash,
    RS: CanonicalSerialize
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
pub fn create_failure_signature_buffer<P,RS>(failure_send_funds: &FailureSendFunds<P,RS>,
                        pending_request: &PendingRequest<P>) -> Vec<u8> 
where
    P: CanonicalSerialize + Eq + std::hash::Hash,
    RS: CanonicalSerialize,
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


/// Verify a failure signature
impl Verify for (&FailureSendFunds<PublicKey, Signature>, &PendingRequest<PublicKey>) {

    fn verify(&self) -> bool {
        let (failure_send_funds, pending_request) = self;

        let failure_signature_buffer = create_failure_signature_buffer(
                                            &failure_send_funds,
                                            &pending_request);
        let reporting_public_key = &failure_send_funds.reporting_public_key;
        // Make sure that the reporting_public_key is on the route:
        // TODO: Should we check that it is after us? Is it checked somewhere else?
        if let None = pending_request.route.pk_to_index(&reporting_public_key) {
            return false;
        }

        if !verify_signature(&failure_signature_buffer, 
                         &reporting_public_key.public_key, 
                         &failure_send_funds.signature.signature) {
            return false;
        }
        true
    }
}

pub fn prepare_receipt<P,RS>(response_send_funds: &ResponseSendFunds<RS>,
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

impl Verify for (&SendFundsReceipt<Signature>, TPublicKey<PublicKey>) {
    fn verify(&self) -> bool {
        let (receipt, public_key) = self;

        let mut data = Vec::new();
        data.extend(FUND_SUCCESS_PREFIX);
        data.extend(receipt.response_hash.as_ref());
        data.extend(receipt.invoice_id.as_ref());
        data.write_u128::<BigEndian>(receipt.dest_payment).unwrap();
        verify_signature(&data, &public_key.public_key, &receipt.signature.signature)
    }
}



// Prefix used for chain hashing of token channel funds.
// NEXT is used for hashing for the next move token funds.
const TOKEN_NEXT: &[u8] = b"NEXT";

/// Combine all operations into one hash value.
pub fn operations_hash<A,P,RS,MS>(move_token: &MoveToken<A,P,RS,MS>) -> HashResult 
where
    P: CanonicalSerialize,
    RS: CanonicalSerialize,
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
pub fn local_address_hash<A,P,RS,MS>(move_token: &MoveToken<A,P,RS,MS>) -> HashResult 
where
    A: CanonicalSerialize,
{
    sha_512_256(&move_token.opt_local_address.canonical_serialize())
}

/// Hash operations and local_address:
pub fn prefix_hash<A,P,RS,MS>(move_token: &MoveToken<A,P,RS,MS>) -> HashResult 
where
    A: CanonicalSerialize,
    P: CanonicalSerialize,
    RS: CanonicalSerialize,
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

pub fn move_token_signature_buff<A,P,RS,MS>(move_token: &MoveToken<A,P,RS,MS>) -> Vec<u8> 
where
    A: CanonicalSerialize,
    P: CanonicalSerialize,
    RS: CanonicalSerialize,
    MS: CanonicalSerialize,
{
    let mut sig_buffer = Vec::new();
    sig_buffer.extend_from_slice(&sha_512_256(TOKEN_NEXT));
    sig_buffer.extend_from_slice(&prefix_hash(move_token));
    sig_buffer.write_u64::<BigEndian>(move_token.inconsistency_counter).unwrap();
    sig_buffer.write_u128::<BigEndian>(move_token.move_token_counter).unwrap();
    sig_buffer.write_i128::<BigEndian>(move_token.balance).unwrap();
    sig_buffer.write_u128::<BigEndian>(move_token.local_pending_debt).unwrap();
    sig_buffer.write_u128::<BigEndian>(move_token.remote_pending_debt).unwrap();
    sig_buffer.extend_from_slice(&move_token.rand_nonce);

    sig_buffer
}

impl<A> Verify for (&MoveToken<A,PublicKey,Signature,Signature>, &TPublicKey<PublicKey>) 
where
    A: CanonicalSerialize,
{
    fn verify(&self) -> bool {
        let (move_token, public_key) = self;
        let sig_buffer = move_token_signature_buff(move_token);
        verify_signature(&sig_buffer, &public_key.public_key, &move_token.new_token.signature)
    }
}

