use byteorder::{BigEndian, WriteBytesExt};

use crypto::hash::{self, sha_512_256};
use crypto::hash_lock::HashLock;
use crypto::identity::verify_signature;

use proto::crypto::{HashResult, InvoiceId, PlainLock, PublicKey};

use common::int_convert::usize_to_u64;

use proto::funder::messages::{
    CollectSendFundsOp, Commit, MoveToken, MultiCommit, PendingTransaction, Receipt,
    ResponseSendFundsOp,
};

use crate::canonical::CanonicalSerialize;

pub trait SignatureBuff {
    /// Obtain a buffer to sign over
    fn signature_buff(&self) -> Vec<u8>;
}

pub trait VerifySignature {
    /// Verify a structure
    fn verify_signature(&self) -> bool;
}

pub const FUNDS_RESPONSE_PREFIX: &[u8] = b"FUND_RESPONSE";
pub const FUNDS_CANCEL_PREFIX: &[u8] = b"FUND_CANCEL";

/// Create the buffer we sign over at the Response funds.
/// Note that the signature is not just over the Response funds bytes. The signed buffer also
/// contains information from the Request funds.
pub fn create_response_signature_buffer<S>(
    response_send_funds: &ResponseSendFundsOp<S>,
    pending_transaction: &PendingTransaction,
) -> Vec<u8> {
    let mut sbuffer = Vec::new();

    sbuffer.extend_from_slice(&hash::sha_512_256(FUNDS_RESPONSE_PREFIX));

    let mut inner_blob = Vec::new();
    inner_blob.extend_from_slice(&pending_transaction.request_id);
    inner_blob.extend_from_slice(&response_send_funds.rand_nonce);

    sbuffer.extend_from_slice(&hash::sha_512_256(&inner_blob));
    sbuffer.extend_from_slice(&pending_transaction.src_hashed_lock);
    sbuffer.extend_from_slice(&response_send_funds.dest_hashed_lock);
    sbuffer
        .write_u128::<BigEndian>(pending_transaction.dest_payment)
        .unwrap();
    sbuffer
        .write_u128::<BigEndian>(pending_transaction.total_dest_payment)
        .unwrap();
    sbuffer.extend_from_slice(&pending_transaction.invoice_id);

    sbuffer
}

pub fn prepare_receipt(
    collect_send_funds: &CollectSendFundsOp,
    response_send_funds: &ResponseSendFundsOp,
    pending_transaction: &PendingTransaction,
) -> Receipt {
    let mut hash_buff = Vec::new();
    hash_buff.extend_from_slice(&pending_transaction.request_id);
    hash_buff.extend_from_slice(&response_send_funds.rand_nonce);
    let response_hash = hash::sha_512_256(&hash_buff);
    // = sha512/256(requestId || randNonce)

    Receipt {
        response_hash,
        invoice_id: pending_transaction.invoice_id.clone(),
        src_plain_lock: collect_send_funds.src_plain_lock.clone(),
        dest_plain_lock: collect_send_funds.dest_plain_lock.clone(),
        dest_payment: pending_transaction.dest_payment.into(),
        total_dest_payment: pending_transaction.total_dest_payment.into(),
        signature: response_send_funds.signature.clone(),
    }
}

/// Verify that a given receipt's signature is valid
pub fn verify_receipt(receipt: &Receipt, public_key: &PublicKey) -> bool {
    let mut data = Vec::new();

    data.extend_from_slice(&hash::sha_512_256(FUNDS_RESPONSE_PREFIX));
    data.extend(receipt.response_hash.as_ref());
    data.extend_from_slice(&receipt.src_plain_lock.hash_lock());
    data.extend_from_slice(&receipt.dest_plain_lock.hash_lock());
    data.write_u128::<BigEndian>(*receipt.dest_payment).unwrap();
    data.write_u128::<BigEndian>(*receipt.total_dest_payment)
        .unwrap();
    data.extend(receipt.invoice_id.as_ref());
    verify_signature(&data, public_key, &receipt.signature)
}

/// Create a Commit (out of band) message given a ResponseSendFunds
pub fn prepare_commit(
    response_send_funds: &ResponseSendFundsOp,
    pending_transaction: &PendingTransaction,
    src_plain_lock: PlainLock,
) -> Commit {
    let mut hash_buff = Vec::new();
    hash_buff.extend_from_slice(&pending_transaction.request_id);
    hash_buff.extend_from_slice(&response_send_funds.rand_nonce);
    let response_hash = hash::sha_512_256(&hash_buff);
    // = sha512/256(requestId || randNonce)

    Commit {
        response_hash,
        dest_payment: pending_transaction.dest_payment.into(),
        src_plain_lock,
        dest_hashed_lock: response_send_funds.dest_hashed_lock.clone(),
        signature: response_send_funds.signature.clone(),
    }
}

/// Verify that a given Commit signature is valid
fn verify_commit(
    commit: &Commit,
    invoice_id: &InvoiceId,
    total_dest_payment: u128,
    local_public_key: &PublicKey,
) -> bool {
    let mut data = Vec::new();

    data.extend_from_slice(&hash::sha_512_256(FUNDS_RESPONSE_PREFIX));
    data.extend(commit.response_hash.as_ref());
    data.extend_from_slice(&commit.src_plain_lock.hash_lock());
    data.extend_from_slice(&commit.dest_hashed_lock);
    data.write_u128::<BigEndian>(*commit.dest_payment).unwrap();
    data.write_u128::<BigEndian>(total_dest_payment).unwrap();
    data.extend(invoice_id.as_ref());
    verify_signature(&data, local_public_key, &commit.signature)
}

// TODO: Possibly split nicely into two functions?
/// Verify that all the Commit-s inside a MultiCommit are valid
pub fn verify_multi_commit(multi_commit: &MultiCommit, local_public_key: &PublicKey) -> bool {
    let mut is_sig_valid = true;
    for commit in &multi_commit.commits {
        // We don't exit immediately on verification failure to get a constant time verification.
        // (Not sure if this is really important here)
        is_sig_valid &= verify_commit(
            commit,
            &multi_commit.invoice_id,
            *multi_commit.total_dest_payment,
            local_public_key,
        );
    }
    if !is_sig_valid {
        return false;
    }

    // Check if the credits add up:
    let mut sum_credits = 0u128;
    for commit in &multi_commit.commits {
        sum_credits = if let Some(sum_credits) = sum_credits.checked_add(*commit.dest_payment) {
            sum_credits
        } else {
            return false;
        }
    }

    // Require that the multi_commit.total_dest_payment matches the sum of all commit.dest_payment:
    sum_credits == *multi_commit.total_dest_payment
}

// Prefix used for chain hashing of token channel funds.
// NEXT is used for hashing for the next move token funds.
pub const TOKEN_NEXT: &[u8] = b"NEXT";

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

    // TODO: Use CanonicalSerialize instead here:
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
    sig_buffer.extend_from_slice(&move_token.local_public_key);
    sig_buffer.extend_from_slice(&move_token.remote_public_key);
    sig_buffer
        .write_u64::<BigEndian>(move_token.inconsistency_counter)
        .unwrap();
    sig_buffer
        .write_u128::<BigEndian>(*move_token.move_token_counter)
        .unwrap();
    sig_buffer
        .write_i128::<BigEndian>(*move_token.balance)
        .unwrap();
    sig_buffer
        .write_u128::<BigEndian>(*move_token.local_pending_debt)
        .unwrap();
    sig_buffer
        .write_u128::<BigEndian>(*move_token.remote_pending_debt)
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
