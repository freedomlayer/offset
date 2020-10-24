use byteorder::{BigEndian, WriteBytesExt};

use crypto::hash;
use crypto::hash_lock::HashLock;
use crypto::identity::verify_signature;

use proto::crypto::PublicKey;

use proto::funder::messages::{Commit, MoveToken, Receipt};
use proto::index_server::messages::MutationsUpdate;

use crate::canonical::CanonicalSerialize;
use crate::signature_buff::{
    create_mutations_update_signature_buff, move_token_signature_buff, FUNDS_RESPONSE_PREFIX,
};

// TODO: Add a local test that makes sure verify_receipt is in sync with verify_commit_signature
/// Verify that a given receipt's signature is valid
pub fn verify_receipt(receipt: &Receipt, public_key: &PublicKey) -> bool {
    let mut data = Vec::new();

    data.extend_from_slice(&hash::hash_buffer(FUNDS_RESPONSE_PREFIX));
    data.extend(receipt.response_hash.as_ref());
    data.extend_from_slice(&receipt.src_plain_lock.hash_lock());
    data.extend_from_slice(&receipt.dest_plain_lock.hash_lock());
    data.extend_from_slice(&receipt.is_complete.canonical_serialize());
    data.write_u128::<BigEndian>(receipt.dest_payment).unwrap();
    data.write_u128::<BigEndian>(receipt.total_dest_payment)
        .unwrap();
    data.extend(receipt.invoice_id.as_ref());
    data.extend_from_slice(&receipt.currency.canonical_serialize());
    verify_signature(&data, public_key, &receipt.signature)
}

/// Verify that a given Commit signature is valid
fn verify_commit_signature(commit: &Commit, local_public_key: &PublicKey) -> bool {
    let mut data = Vec::new();

    data.extend_from_slice(&hash::hash_buffer(FUNDS_RESPONSE_PREFIX));
    data.extend(commit.response_hash.as_ref());
    data.extend_from_slice(&commit.src_plain_lock.hash_lock());
    data.extend_from_slice(&commit.dest_hashed_lock);
    let is_complete = true;
    data.extend_from_slice(&is_complete.canonical_serialize());
    data.write_u128::<BigEndian>(commit.dest_payment).unwrap();
    data.write_u128::<BigEndian>(commit.total_dest_payment)
        .unwrap();
    data.extend(commit.invoice_id.as_ref());
    data.extend_from_slice(&commit.currency.canonical_serialize());
    verify_signature(&data, local_public_key, &commit.signature)
}

/// Verify a Commit message
pub fn verify_commit(commit: &Commit, local_public_key: &PublicKey) -> bool {
    // Make sure that the relationship between dest_payment and total_dest_payment makes sense:
    if commit.total_dest_payment < commit.dest_payment {
        return false;
    }

    // Verify signature:
    verify_commit_signature(commit, local_public_key)
}

/// Verify that new_token is a valid signature over the rest of the fields.
pub fn verify_move_token<B>(move_token: &MoveToken<B>, public_key: &PublicKey) -> bool
where
    B: CanonicalSerialize + Clone,
{
    let sig_buffer = move_token_signature_buff(move_token);
    verify_signature(&sig_buffer, public_key, &move_token.new_token)
}

/// Verify the signature at the MutationsUpdate structure.
/// Note that this structure also contains the `node_public_key` field, which is the identity
/// of the node who signed this struct.
pub fn verify_mutations_update(mutations_update: &MutationsUpdate) -> bool {
    let signature_buff = create_mutations_update_signature_buff(&mutations_update);
    verify_signature(
        &signature_buff,
        &mutations_update.node_public_key,
        &mutations_update.signature,
    )
}
