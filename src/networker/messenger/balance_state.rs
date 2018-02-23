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
    ResetChannel(i64), // new_balanace
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

struct IncomingRequestSendMessage {
}

struct IncomingResponseSendMessage {
}

enum ProcessTransOutput {
    Request(IncomingRequestSendMessage),
    Response(IncomingResponseSendMessage),
}

pub struct ProcessTransListOutput {
    requests: Vec<IncomingRequestSendMessage>,
    responses: Vec<IncomingResponseSendMessage>,
}

#[derive(Debug)]
pub enum ProcessTransError {
    TempToCompile,
    RemoteMaxDebtTooLarge(u64),
    InvoiceIdExists,
}

#[derive(Debug)]
pub struct ProcessTransListError {
    index: usize,
    process_trans_error: ProcessTransError,
}

fn process_set_remote_max_debt(mut trans_balance_state: TransBalanceState,
                                   proposed_max_debt: u64)
                                    -> (TransBalanceState, Result<(), ProcessTransError>) {

    let credit_state = &trans_balance_state.credit_state;
    let max_local_max_debt: u64 = cmp::min(
        MAX_NEIGHBOR_DEBT as i64, 
        (credit_state.local_pending_debt as i64) - credit_state.balance) as u64;

    if proposed_max_debt > max_local_max_debt {
        (trans_balance_state, 
         Err(ProcessTransError::RemoteMaxDebtTooLarge(proposed_max_debt)))
    } else {
        trans_balance_state.credit_state.local_max_debt = proposed_max_debt;
        (trans_balance_state, Ok(()))
    }
}

fn process_set_invoice_id(mut trans_balance_state: TransBalanceState,
                                   invoice_id: &InvoiceId)
                                    -> (TransBalanceState, Result<(), ProcessTransError>) {

    let remote_invoice_id = &mut trans_balance_state.credit_state.remote_invoice_id;
    *remote_invoice_id = match *remote_invoice_id {
        None => Some(invoice_id.clone()),
        Some(_) => return (trans_balance_state, 
                           Err(ProcessTransError::InvoiceIdExists)),
    };
    (trans_balance_state, Ok(()))
}

fn process_load_funds(trans_balance_state: TransBalanceState,
                                   send_funds_receipt: &SendFundsReceipt)
                                    -> (TransBalanceState, Result<(), ProcessTransError>) {
    unreachable!();
}

fn process_request_send_message(trans_balance_state: TransBalanceState,
                                   trans_list_output: &mut ProcessTransListOutput,
                                   request_send_msg: &RequestSendMessage)
                                    -> (TransBalanceState, Result<(), ProcessTransError>) {
    unreachable!();
}

fn process_response_send_message(trans_balance_state: TransBalanceState,
                                   trans_list_output: &mut ProcessTransListOutput,
                                   response_send_msg: &ResponseSendMessage)
                                    -> (TransBalanceState, Result<(), ProcessTransError>) {
    unreachable!();
}

fn process_failed_send_message(trans_balance_state: TransBalanceState,
                                   trans_list_output: &mut ProcessTransListOutput,
                                   failed_send_msg: &FailedSendMessage)
                                    -> (TransBalanceState, Result<(), ProcessTransError>) {
    unreachable!();
}

fn process_reset_channel(trans_balance_state: TransBalanceState,
                                   new_balance: i64)
                                    -> (TransBalanceState, Result<(), ProcessTransError>) {
    unreachable!();
}

fn process_trans(trans_balance_state: TransBalanceState, 
                 trans: &NetworkerTCTransaction,
                 mut trans_list_output: &mut ProcessTransListOutput)
                    -> (TransBalanceState, Result<(), ProcessTransError>) {

    match *trans {
        NetworkerTCTransaction::SetRemoteMaxDebt(proposed_max_debt) => 
            process_set_remote_max_debt(trans_balance_state,
                                        proposed_max_debt),
        NetworkerTCTransaction::SetInvoiceId(ref rand_nonce) =>
            process_set_invoice_id(trans_balance_state,
                                     rand_nonce),
        NetworkerTCTransaction::LoadFunds(ref send_funds_receipt) => 
            process_load_funds(trans_balance_state,
                               send_funds_receipt),
        NetworkerTCTransaction::RequestSendMessage(ref request_send_msg) =>
            process_request_send_message(trans_balance_state,
                                         &mut trans_list_output,
                                         request_send_msg),
        NetworkerTCTransaction::ResponseSendMessage(ref response_send_msg) =>
            process_response_send_message(trans_balance_state,
                                          &mut trans_list_output,
                                          response_send_msg),
        NetworkerTCTransaction::FailedSendMessage(ref failed_send_msg) => 
            process_failed_send_message(trans_balance_state,
                                        &mut trans_list_output,
                                        failed_send_msg),
        NetworkerTCTransaction::ResetChannel(new_balance) => 
            process_reset_channel(trans_balance_state,
                                  new_balance),
    }
}

fn process_trans_list(transactions: &[NetworkerTCTransaction], 
    mut trans_balance_state: TransBalanceState)
                        -> (TransBalanceState, 
                            Result<ProcessTransListOutput, ProcessTransListError>) {

    let mut trans_list_output = ProcessTransListOutput {
        requests: Vec::new(),
        responses: Vec::new(),
    };

    for (index, trans) in transactions.into_iter().enumerate() {
        trans_balance_state = match process_trans(trans_balance_state, trans, 
                                                  &mut trans_list_output) {
            (tbs, Err(e)) => return (tbs, Err(ProcessTransListError {
                index, 
                process_trans_error: e
            })),
            (tbs, Ok(())) => {
                tbs
            },
        }
    }
    (trans_balance_state, Ok(trans_list_output))
}

pub fn atomic_process_trans_list(balance_state: BalanceState,
                             transactions: &[NetworkerTCTransaction])
    -> (BalanceState, Result<ProcessTransListOutput, ProcessTransListError>) {

    let trans_balance_state = TransBalanceState::new(balance_state);
    match process_trans_list(transactions, trans_balance_state) {
        (tbs, Ok(out)) => (tbs.commit(), Ok(out)),
        (tbs, Err(e)) => (tbs.cancel(), Err(e)),
    }
}

