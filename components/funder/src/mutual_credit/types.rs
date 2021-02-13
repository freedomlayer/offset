use common::async_rpc::AsyncOpResult;
use common::u256::U256;

use proto::crypto::{HashResult, HashedLock, PlainLock, PublicKey, Signature, Uid};
use proto::funder::messages::{
    CancelSendFundsOp, McBalance, RequestSendFundsOp, ResponseSendFundsOp,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingTransaction {
    /// Id number of this request. Used to identify the whole transaction
    /// over this route.
    pub request_id: Uid,
    /// A hash lock created by the originator of this request
    pub src_hashed_lock: HashedLock,
    /// Amount paid to destination
    pub dest_payment: u128,
    /// hash(hash(actionId) || hash(totalDestPayment) || hash(description) || hash(additional))
    /// TODO: Check if this scheme is safe? Do we need to use pbkdf instead?
    pub invoice_hash: HashResult,
    /// List of next nodes to transfer this request
    pub route: Vec<PublicKey>,
    /// Amount of fees left to give to mediators
    /// Every mediator takes the amount of fees he wants and subtracts this
    /// value accordingly.
    pub left_fees: u128,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McRequest {
    /// Id number of this request. Used to identify the whole transaction
    /// over this route.
    pub request_id: Uid,
    /// A hash lock created by the originator of this request
    pub src_hashed_lock: HashedLock,
    /// Amount paid to destination
    pub dest_payment: u128,
    /// hash(hash(actionId) || hash(totalDestPayment) || hash(description) || hash(additional))
    /// TODO: Check if this scheme is safe? Do we need to use pbkdf instead?
    pub invoice_hash: HashResult,
    /// List of next nodes to transfer this request
    pub route: Vec<PublicKey>,
    /// Amount of fees left to give to mediators
    /// Every mediator takes the amount of fees he wants and subtracts this
    /// value accordingly.
    pub left_fees: u128,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McResponse {
    /// Id number of this request. Used to identify the whole transaction
    /// over this route.
    pub request_id: Uid,
    pub src_plain_lock: PlainLock,
    /// Serial number used for this collection of invoice money.
    /// This should be a u128 counter, increased by 1 for every collected
    /// invoice.
    pub serial_num: u128,
    /// Signature{key=destinationKey}(
    ///   hash("FUNDS_RESPONSE") ||
    ///   hash(request_id || src_plain_lock || dest_payment) ||
    ///   hash(currency) ||
    ///   serialNum ||
    ///   invoiceHash)
    /// )
    pub signature: Signature,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McCancel {
    /// Id number of this request. Used to identify the whole transaction
    /// over this route.
    pub request_id: Uid,
}

#[derive(Debug, Clone)]
pub enum McOp {
    Request(McRequest),
    Response(McResponse),
    Cancel(McCancel),
}

pub trait McDbClient {
    fn get_balance(&mut self) -> AsyncOpResult<McBalance>;
    fn set_balance(&mut self, new_balance: i128) -> AsyncOpResult<()>;
    fn set_local_pending_debt(&mut self, debt: u128) -> AsyncOpResult<()>;
    fn set_remote_pending_debt(&mut self, debt: u128) -> AsyncOpResult<()>;
    fn set_in_fees(&mut self, in_fees: U256) -> AsyncOpResult<()>;
    fn set_out_fees(&mut self, out_fees: U256) -> AsyncOpResult<()>;
    fn get_local_pending_transaction(
        &mut self,
        request_id: Uid,
    ) -> AsyncOpResult<Option<PendingTransaction>>;
    fn insert_local_pending_transaction(
        &mut self,
        pending_transaction: PendingTransaction,
    ) -> AsyncOpResult<()>;
    fn remove_local_pending_transaction(&mut self, request_id: Uid) -> AsyncOpResult<()>;
    fn get_remote_pending_transaction(
        &mut self,
        request_id: Uid,
    ) -> AsyncOpResult<Option<PendingTransaction>>;
    fn insert_remote_pending_transaction(
        &mut self,
        pending_transaction: PendingTransaction,
    ) -> AsyncOpResult<()>;
    fn remove_remote_pending_transaction(&mut self, request_id: Uid) -> AsyncOpResult<()>;
}

impl From<McRequest> for PendingTransaction {
    fn from(mc_request: McRequest) -> Self {
        Self {
            request_id: mc_request.request_id,
            src_hashed_lock: mc_request.src_hashed_lock,
            dest_payment: mc_request.dest_payment,
            invoice_hash: mc_request.invoice_hash,
            route: mc_request.route,
            left_fees: mc_request.left_fees,
        }
    }
}

impl From<RequestSendFundsOp> for McRequest {
    fn from(request_send_funds: RequestSendFundsOp) -> Self {
        Self {
            request_id: request_send_funds.request_id,
            src_hashed_lock: request_send_funds.src_hashed_lock,
            dest_payment: request_send_funds.dest_payment,
            invoice_hash: request_send_funds.invoice_hash,
            route: request_send_funds.route,
            left_fees: request_send_funds.left_fees,
        }
    }
}

impl From<ResponseSendFundsOp> for McResponse {
    fn from(response_send_funds: ResponseSendFundsOp) -> Self {
        Self {
            request_id: response_send_funds.request_id,
            src_plain_lock: response_send_funds.src_plain_lock,
            serial_num: response_send_funds.serial_num,
            signature: response_send_funds.signature,
        }
    }
}

impl From<CancelSendFundsOp> for McCancel {
    fn from(cancel_send_funds: CancelSendFundsOp) -> Self {
        Self {
            request_id: cancel_send_funds.request_id,
        }
    }
}
