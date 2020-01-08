use crypto::hash;
use proto::crypto::PlainLock;
use proto::funder::messages::{
    CollectSendFundsOp, Commit, Currency, PendingTransaction, Receipt, ResponseSendFundsOp,
    TransactionStage,
};

pub fn prepare_receipt(
    currency: &Currency,
    collect_send_funds: &CollectSendFundsOp,
    response_send_funds: &ResponseSendFundsOp,
    pending_transaction: &PendingTransaction,
) -> Receipt {
    let mut hash_buff = Vec::new();
    hash_buff.extend_from_slice(&pending_transaction.request_id);
    hash_buff.extend_from_slice(&response_send_funds.rand_nonce);
    let response_hash = hash::sha_512_256(&hash_buff);
    // = sha512/256(requestId || randNonce)

    let is_complete = match &pending_transaction.stage {
        TransactionStage::Response(_dest_hashed_lock, is_complete) => *is_complete,
        _ => unreachable!(),
    };

    Receipt {
        response_hash,
        invoice_id: pending_transaction.invoice_id.clone(),
        currency: currency.clone(),
        src_plain_lock: collect_send_funds.src_plain_lock.clone(),
        dest_plain_lock: collect_send_funds.dest_plain_lock.clone(),
        is_complete,
        dest_payment: pending_transaction.dest_payment,
        total_dest_payment: pending_transaction.total_dest_payment,
        signature: response_send_funds.signature.clone(),
    }
}

/// Create a Commit (out of band) message given a ResponseSendFunds
pub fn prepare_commit(
    currency: Currency,
    response_send_funds: &ResponseSendFundsOp,
    pending_transaction: &PendingTransaction,
    src_plain_lock: PlainLock,
) -> Commit {
    assert!(response_send_funds.is_complete);

    let mut hash_buff = Vec::new();
    hash_buff.extend_from_slice(&pending_transaction.request_id);
    hash_buff.extend_from_slice(&response_send_funds.rand_nonce);
    let response_hash = hash::sha_512_256(&hash_buff);
    // = sha512/256(requestId || randNonce)

    Commit {
        response_hash,
        src_plain_lock,
        dest_hashed_lock: response_send_funds.dest_hashed_lock.clone(),
        dest_payment: pending_transaction.dest_payment,
        total_dest_payment: pending_transaction.total_dest_payment,
        invoice_id: pending_transaction.invoice_id.clone(),
        currency,
        signature: response_send_funds.signature.clone(),
    }
}
