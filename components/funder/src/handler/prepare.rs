use crypto::hash;
use proto::crypto::PlainLock;
use proto::funder::messages::{
    CollectSendFundsOp, Commit, PendingTransaction, Receipt, ResponseSendFundsOp,
};

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
