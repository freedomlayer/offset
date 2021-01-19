use proto::funder::messages::{Currency, PendingTransaction, ResponseSendFundsOp};

use crate::mutual_credit::types::{McRequest, McResponse};

pub fn pending_transaction_from_mc_request(
    request: McRequest,
    currency: Currency,
) -> PendingTransaction {
    PendingTransaction {
        request_id: request.request_id,
        currency,
        src_hashed_lock: request.src_hashed_lock,
        dest_payment: request.dest_payment,
        invoice_hash: request.invoice_hash,
        route: request.route,
        left_fees: request.left_fees,
    }
}

pub fn response_op_from_mc_response(response: McResponse) -> ResponseSendFundsOp {
    ResponseSendFundsOp {
        request_id: response.request_id,
        src_plain_lock: response.src_plain_lock,
        serial_num: response.serial_num,
        signature: response.signature,
    }
}
