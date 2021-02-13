use proto::funder::messages::{Currency, ResponseSendFundsOp};

use signature::signature_buff::create_response_signature_buffer;

use crate::mutual_credit::types::{McRequest, McResponse, PendingTransaction};

/*
pub fn pending_transaction_from_mc_request(request: McRequest) -> PendingTransaction {
    PendingTransaction {
        request_id: request.request_id,
        src_hashed_lock: request.src_hashed_lock,
        dest_payment: request.dest_payment,
        invoice_hash: request.invoice_hash,
    }
}
*/

pub fn response_op_from_mc_response(response: McResponse) -> ResponseSendFundsOp {
    ResponseSendFundsOp {
        request_id: response.request_id,
        src_plain_lock: response.src_plain_lock,
        serial_num: response.serial_num,
        signature: response.signature,
    }
}

/// Calculate a response signature
pub fn mc_response_signature_buffer(
    currency: &Currency,
    mc_response: &McResponse,
    pending_transaction: &PendingTransaction,
) -> Vec<u8> {
    create_response_signature_buffer(
        &pending_transaction.request_id,
        currency,
        pending_transaction.dest_payment,
        &pending_transaction.invoice_hash,
        &mc_response.src_plain_lock,
        mc_response.serial_num,
    )
}
