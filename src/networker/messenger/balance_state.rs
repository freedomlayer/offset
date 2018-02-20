use std::collections::HashMap;

use proto::indexer::NeighborsRoute;
use crypto::rand_values::RandValue;
use crypto::uid::Uid;
use crypto::identity::{Signature, PublicKey};
use proto::funder::InvoiceId;
use proto::common::SendFundsReceipt;
use super::super::messages::PendingNeighborRequest;
use utils::trans_hashmap::TransHashMap;

pub enum NetworkerTCTransaction {
    SetRemoteMaximumDebt(u64),
    FundsRandNonce(Uid),
    LoadFunds(SendFundsReceipt),
    RequestSendMessage {
        request_id: Uid,
        route: NeighborsRoute,
        request_content: Vec<u8>,
        max_response_len: u32,
        processing_fee_proposal: u64,
        credits_per_byte_proposal: u64,
    },
    ResponseSendMessage {
        request_id: Uid,
        rand_nonce: RandValue,
        processing_fee_collected: u64,
        response_content: Vec<u8>,
        signature: Signature,
    },
    FailedSendMessage {
        request_id: Uid,
        reporting_public_key: PublicKey,
        rand_nonce: RandValue,
        signature: Signature,
    },
    ResetChannel {
        new_balance: i64,
    },
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
}

#[derive(Debug)]
pub struct ProcessTransListError {
    index: usize,
    process_trans_error: ProcessTransError,
}

fn process_trans(trans: &NetworkerTCTransaction,
                trans_balance_state: TransBalanceState)
                    -> (TransBalanceState, Result<ProcessTransOutput, ProcessTransError>) {

    // TODO: Implement this
    assert!(false);
    (trans_balance_state, Err(ProcessTransError::TempToCompile))
}

fn process_trans_list(transactions: &[NetworkerTCTransaction], 
    mut trans_balance_state: TransBalanceState)
                        -> (TransBalanceState, Result<ProcessTransListOutput, ProcessTransListError>) {

    let mut trans_list_output = ProcessTransListOutput {
        requests: Vec::new(),
        responses: Vec::new(),
    };

    for (index, trans) in transactions.into_iter().enumerate() {
        trans_balance_state = match process_trans(trans, trans_balance_state) {
            (tbs, Err(e)) => return (tbs, Err(ProcessTransListError {
                index, 
                process_trans_error: e
            })),
            (tbs, Ok(ProcessTransOutput::Request(incoming_request))) => {
                trans_list_output.requests.push(incoming_request);
                tbs
            },
            (tbs, Ok(ProcessTransOutput::Response(incoming_response))) => {
                trans_list_output.responses.push(incoming_response);
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

