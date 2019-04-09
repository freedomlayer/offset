use byteorder::{BigEndian, WriteBytesExt};

use crypto::hash::sha_512_256;
use crypto::identity::{verify_signature, PublicKey};

use crate::funder::signature_buff::TOKEN_NEXT;
use crate::report::messages::MoveTokenHashedReport;

// TODO: How to keep this function in sync with move_token_signature_buff from
// funder::signature_buff?
fn move_token_hashed_report_signature_buff(
    move_token_hashed_report: &MoveTokenHashedReport,
) -> Vec<u8> {
    let mut sig_buffer = Vec::new();
    sig_buffer.extend_from_slice(&sha_512_256(TOKEN_NEXT));
    sig_buffer.extend_from_slice(&move_token_hashed_report.prefix_hash);
    sig_buffer.extend_from_slice(&move_token_hashed_report.local_public_key);
    sig_buffer.extend_from_slice(&move_token_hashed_report.remote_public_key);
    sig_buffer
        .write_u64::<BigEndian>(move_token_hashed_report.inconsistency_counter)
        .unwrap();
    sig_buffer
        .write_u128::<BigEndian>(move_token_hashed_report.move_token_counter)
        .unwrap();
    sig_buffer
        .write_i128::<BigEndian>(move_token_hashed_report.balance)
        .unwrap();
    sig_buffer
        .write_u128::<BigEndian>(move_token_hashed_report.local_pending_debt)
        .unwrap();
    sig_buffer
        .write_u128::<BigEndian>(move_token_hashed_report.remote_pending_debt)
        .unwrap();
    sig_buffer.extend_from_slice(&move_token_hashed_report.rand_nonce);

    sig_buffer
}

/// Verify that new_token is a valid signature over the rest of the fields.
pub fn verify_move_token_hashed_report<B>(
    move_token_hashed_report: &MoveTokenHashedReport,
    public_key: &PublicKey,
) -> bool {
    let sig_buffer = move_token_hashed_report_signature_buff(move_token_hashed_report);
    verify_signature(&sig_buffer, public_key, &move_token_hashed_report.new_token)
}
