use std::cmp;
use std::collections::HashMap;

use proto::indexer::NeighborsRoute;
use crypto::rand_values::RandValue;
use crypto::uid::Uid;
use crypto::identity::{Signature, PublicKey};
use proto::funder::InvoiceId;
use proto::common::SendFundsReceipt;
use super::super::messages::PendingNeighborRequest;
use utils::trans_hashmap::TransHashMap;

const MAX_NEIGHBOR_DEBT: u64 = (1 << 63) - 1;

pub struct RequestSendMessage {
    request_id: Uid,
    route: NeighborsRoute,
    request_content: Vec<u8>,
    max_response_len: u32,
    processing_fee_proposal: u64,
    credits_per_byte_proposal: u64,
}

pub struct ResponseSendMessage {
    request_id: Uid,
    rand_nonce: RandValue,
    processing_fee_collected: u64,
    response_content: Vec<u8>,
    signature: Signature,
}

pub struct FailedSendMessage {
    request_id: Uid,
    reporting_public_key: PublicKey,
    rand_nonce: RandValue,
    signature: Signature,
}


pub enum NetworkerTCTransaction {
    SetRemoteMaxDebt(u64),
    SetInvoiceId(InvoiceId),
    LoadFunds(SendFundsReceipt),
    RequestSendMessage(RequestSendMessage),
    ResponseSendMessage(ResponseSendMessage), 
    FailedSendMessage(FailedSendMessage),
    // ResetChannel(i64), // new_balanace
}

#[derive(Clone)]
struct CreditState {
    pub remote_max_debt: u64,
    pub local_max_debt: u64,
    pub remote_pending_debt: u64,
    pub local_pending_debt: u64,
    pub balance: i64,
    pub local_invoice_id: Option<InvoiceId>,
    pub remote_invoice_id: Option<InvoiceId>,
}

pub struct BalanceState {
    credit_state: CreditState,
    pending_local_requests: HashMap<Uid, PendingNeighborRequest>,
    pending_remote_requests: HashMap<Uid, PendingNeighborRequest>,
}

struct TransBalanceState {
    credit_state: CreditState,
    orig_credit_state: CreditState,
    tp_local_requests: TransHashMap<Uid, PendingNeighborRequest>,
    tp_remote_requests: TransHashMap<Uid, PendingNeighborRequest>,
}

impl TransBalanceState {
    pub fn new(balance_state: BalanceState) -> Self {
        TransBalanceState {
            credit_state: balance_state.credit_state.clone(),
            orig_credit_state: balance_state.credit_state,
            tp_local_requests: TransHashMap::new(balance_state.pending_local_requests),
            tp_remote_requests: TransHashMap::new(balance_state.pending_remote_requests),
        }
    }

    pub fn commit(self) -> BalanceState {
        BalanceState {
            credit_state: self.credit_state,
            pending_local_requests: self.tp_local_requests.commit(),
            pending_remote_requests: self.tp_remote_requests.commit(),
        }
    }

    pub fn cancel(self) -> BalanceState {
        BalanceState {
            credit_state: self.orig_credit_state,
            pending_local_requests: self.tp_local_requests.cancel(),
            pending_remote_requests: self.tp_remote_requests.cancel(),
        }
    }
}

pub struct IncomingRequestSendMessage {
}

pub struct IncomingResponseSendMessage {
}

pub struct IncomingFailedSendMessage {
}

pub enum ProcessTransOutput {
    Request(IncomingRequestSendMessage),
    Response(IncomingResponseSendMessage),
    Failure(IncomingFailedSendMessage),
}


#[derive(Debug)]
pub enum ProcessTransError {
    RemoteMaxDebtTooLarge(u64),
    InvoiceIdExists,
    MissingInvoiceId,
    InvalidInvoiceId,
    InvalidFundsReceipt,
}

#[derive(Debug)]
pub struct ProcessTransListError {
    index: usize,
    process_trans_error: ProcessTransError,
}

fn process_set_remote_max_debt(mut trans_balance_state: TransBalanceState,
                                   proposed_max_debt: u64)
                                    -> (TransBalanceState, 
                                        Result<Option<ProcessTransOutput>, ProcessTransError>) {

    let credit_state = &trans_balance_state.credit_state;
    let max_local_max_debt: u64 = cmp::min(
        MAX_NEIGHBOR_DEBT as i64, 
        (credit_state.local_pending_debt as i64) - credit_state.balance) as u64;

    if proposed_max_debt > max_local_max_debt {
        (trans_balance_state, 
         Err(ProcessTransError::RemoteMaxDebtTooLarge(proposed_max_debt)))
    } else {
        trans_balance_state.credit_state.local_max_debt = proposed_max_debt;
        (trans_balance_state, Ok(None))
    }
}

fn process_set_invoice_id(mut trans_balance_state: TransBalanceState,
                          invoice_id: &InvoiceId)
                                    -> (TransBalanceState, 
                                        Result<Option<ProcessTransOutput>, ProcessTransError>) {

    let remote_invoice_id = &mut trans_balance_state.credit_state.remote_invoice_id;
    *remote_invoice_id = match *remote_invoice_id {
        None => Some(invoice_id.clone()),
        Some(_) => return (trans_balance_state, 
                           Err(ProcessTransError::InvoiceIdExists)),
    };
    (trans_balance_state, Ok(None))
}


fn process_load_funds(mut trans_balance_state: TransBalanceState,
                      local_public_key: &PublicKey,
                      send_funds_receipt: &SendFundsReceipt)
                        -> (TransBalanceState, 
                            Result<Option<ProcessTransOutput>, ProcessTransError>) {
    // Verify signature:
    if !send_funds_receipt.verify(local_public_key) {
        return (trans_balance_state, Err(ProcessTransError::InvalidFundsReceipt))
    }

    // Make sure that the invoice_id matches the one we have:
    match &trans_balance_state.credit_state.local_invoice_id {
        &Some(ref local_invoice_id) => {
            if local_invoice_id != &send_funds_receipt.invoice_id {
                return (trans_balance_state, Err(ProcessTransError::InvalidInvoiceId));
            }
        },
        &None => return (trans_balance_state, Err(ProcessTransError::MissingInvoiceId)),
    };

    // Possibly trim payment so that: local_pending_debt - balance < MAX_NEIGHBOR_DEBT
    // This means that the sender of payment is losing some credits in this transaction.
    let credit_state = &mut trans_balance_state.credit_state;
    let max_payment = cmp::max(MAX_NEIGHBOR_DEBT as i64 - 
        credit_state.local_pending_debt as i64 + 
        credit_state.balance as i64, 0) as u64;
    let payment = cmp::min(send_funds_receipt.payment, max_payment as u128) as u64;

    // Apply payment to balance:
    credit_state.balance -= payment as i64;

    // Possibly increase local_max_debt if we got too many credits:
    credit_state.local_max_debt = cmp::max(
        credit_state.local_max_debt as i64, 
        credit_state.local_pending_debt as i64 - credit_state.balance as i64)
            as u64;

    (trans_balance_state, Ok(None))
}

#[allow(unused)] //TODO(a4vision): implement.
fn process_request_send_message(trans_balance_state: TransBalanceState,
                                    local_public_key: &PublicKey,
                                    remote_public_key: &PublicKey,
                                   request_send_msg: &RequestSendMessage)
                                    -> (TransBalanceState, 
                                        Result<Option<ProcessTransOutput>, ProcessTransError>) {
    // TODO:
    // - Make sure that route contains 
    //  (remote_public_key -> local_public_key) in this order.
    //
    // - Make sure it is possible to increase remote_max_debt, and then increase it.
    //
    // - Check if request_id is not already inside pending_remote_requests.
    //   If not, insert into pending_remote_requests.
    //
    // - Output a Some(IncomingRequestSendMessage)
    unreachable!();
}

#[allow(unused)] //TODO(a4vision): implement.
fn process_response_send_message(trans_balance_state: TransBalanceState,
                                   response_send_msg: &ResponseSendMessage)
                                    -> (TransBalanceState, 
                                        Result<Option<ProcessTransOutput>, ProcessTransError>) {
    unreachable!();
}

#[allow(unused)] //TODO(a4vision): implement.
fn process_failed_send_message(trans_balance_state: TransBalanceState,
                                   failed_send_msg: &FailedSendMessage)
                                    -> (TransBalanceState, 
                                        Result<Option<ProcessTransOutput>, ProcessTransError>) {
    unreachable!();
}

/*
fn process_reset_channel(mut trans_balance_state: TransBalanceState,
                         local_public_key: &PublicKey,
                         remote_public_key: &PublicKey,
                         trans_list_output: &mut TransListOutput,
                         new_balance: i64)
                            -> (TransBalanceState, Result<(), ProcessTransError>) {

    let credit_state = &mut trans_balance_state.credit_state;
    let expected_new_balance = credit_state.balance 
        + credit_state.remote_pending_debt as i64
        - credit_state.local_pending_debt as i64;

    if new_balance == expected_new_balance {
        credit_state.balance = new_balance;
        credit_state.remote_pending_debt = 0;
        credit_state.local_pending_debt = 0;

    }

    unreachable!();
}
*/

fn process_trans(trans_balance_state: TransBalanceState, 
                 local_public_key: &PublicKey,
                 remote_public_key: &PublicKey,
                 trans: &NetworkerTCTransaction)
                    -> (TransBalanceState, 
                        Result<Option<ProcessTransOutput>, ProcessTransError>) {

    match *trans {
        NetworkerTCTransaction::SetRemoteMaxDebt(proposed_max_debt) => 
            process_set_remote_max_debt(trans_balance_state,
                                        proposed_max_debt),
        NetworkerTCTransaction::SetInvoiceId(ref rand_nonce) =>
            process_set_invoice_id(trans_balance_state,
                                     rand_nonce),
        NetworkerTCTransaction::LoadFunds(ref send_funds_receipt) => 
            process_load_funds(trans_balance_state,
                               local_public_key,
                               send_funds_receipt),
        NetworkerTCTransaction::RequestSendMessage(ref request_send_msg) =>
            process_request_send_message(trans_balance_state,
                                         local_public_key,
                                         remote_public_key,
                                         request_send_msg),
        NetworkerTCTransaction::ResponseSendMessage(ref response_send_msg) =>
            process_response_send_message(trans_balance_state,
                                          response_send_msg),
        NetworkerTCTransaction::FailedSendMessage(ref failed_send_msg) => 
            process_failed_send_message(trans_balance_state,
                                        failed_send_msg),
    }
}

fn process_trans_list(mut trans_balance_state: TransBalanceState, 
                      local_public_key: &PublicKey,
                      remote_public_key: &PublicKey,
                      transactions: &[NetworkerTCTransaction])
                        -> (TransBalanceState, 
                            Result<Vec<ProcessTransOutput>, ProcessTransListError>) {

    let mut trans_list_output = Vec::new();

    for (index, trans) in transactions.into_iter().enumerate() {
        trans_balance_state = match process_trans(trans_balance_state,
                                                  local_public_key,
                                                  remote_public_key,
                                                  trans) {
            (tbs, Err(e)) => return (tbs, Err(ProcessTransListError {
                index, 
                process_trans_error: e
            })),
            (tbs, Ok(Some(trans_output))) => {
                trans_list_output.push(trans_output);
                tbs
            },
            (tbs, Ok(None)) => {
                tbs
            },
        }
    }
    (trans_balance_state, Ok(trans_list_output))
}

pub fn atomic_process_trans_list(balance_state: BalanceState,
                                 local_public_key: &PublicKey,
                                 remote_public_key: &PublicKey,
                                 transactions: &[NetworkerTCTransaction])
    -> (BalanceState, Result<Vec<ProcessTransOutput>, ProcessTransListError>) {

    let trans_balance_state = TransBalanceState::new(balance_state);
    match process_trans_list(trans_balance_state, 
                             local_public_key, 
                             remote_public_key,
                             transactions) {
        (tbs, Ok(out)) => (tbs.commit(), Ok(out)),
        (tbs, Err(e)) => (tbs.cancel(), Err(e)),
    }
}

