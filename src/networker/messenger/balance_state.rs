use std::mem;
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
pub struct CreditState {
    pub remote_max_debt: u64,
    pub local_max_debt: u64,
    pub remote_pending_debt: u64,
    pub local_pending_debt: u64,
    pub balance: i64,
    pub local_invoice_id: Option<InvoiceId>,
    pub remote_invoice_id: Option<InvoiceId>,
}

pub struct BalanceState {
    pub credit_state: CreditState,
    pub pending_local_requests: HashMap<Uid, PendingNeighborRequest>,
    pub pending_remote_requests: HashMap<Uid, PendingNeighborRequest>,
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
                credit_state: &mut CreditState,
                t_pending_local_requests: &mut TransHashMap<Uid, PendingNeighborRequest>,
                t_pending_remote_requests: &mut TransHashMap<Uid, PendingNeighborRequest>)
                    -> Result<ProcessTransOutput, ProcessTransError> {

    // TODO: Implement this
    assert!(false);
    Err(ProcessTransError::TempToCompile)
}

fn process_trans_list(transactions: &[NetworkerTCTransaction], 
    credit_state: &mut CreditState,
    t_pending_local_requests: &mut TransHashMap<Uid, PendingNeighborRequest>,
    t_pending_remote_requests: &mut TransHashMap<Uid, PendingNeighborRequest>)
                        -> Result<ProcessTransListOutput, ProcessTransListError> {

    let mut trans_list_output = ProcessTransListOutput {
        requests: Vec::new(),
        responses: Vec::new(),
    };
    for (index, trans) in transactions.into_iter().enumerate() {
        match process_trans(trans, credit_state, 
                            t_pending_local_requests, t_pending_remote_requests) {
            Err(e) => return Err(ProcessTransListError {
                index, 
                process_trans_error: e
            }),
            Ok(ProcessTransOutput::Request(incoming_request)) => {
                trans_list_output.requests.push(incoming_request);
            },
            Ok(ProcessTransOutput::Response(incoming_response)) => {
                trans_list_output.responses.push(incoming_response);
            },
        }
    }
    Ok(trans_list_output)
}

impl BalanceState {
    pub fn process_trans_list_atomic(&mut self, transactions: &[NetworkerTCTransaction]) 
        -> Result<ProcessTransListOutput, ProcessTransListError> {

        let mut credit_state = self.credit_state.clone();
        let pending_local_requests = mem::replace(&mut self.pending_local_requests, HashMap::new());
        let pending_remote_requests = mem::replace(&mut self.pending_remote_requests, HashMap::new());

        let mut tpl_requests = TransHashMap::new(pending_local_requests);
        let mut tpr_requests = TransHashMap::new(pending_remote_requests);

        // Process all transactions. 
        match process_trans_list(transactions, 
                                       &mut credit_state, 
                                       &mut tpl_requests, 
                                       &mut tpr_requests) {
            Ok(out) => {
                // Processing was successful. 
                // We commit changes to the state.
                self.pending_local_requests = tpl_requests.commit();
                self.pending_remote_requests = tpr_requests.commit();
                self.credit_state = credit_state;
                Ok(out)
            },
            Err(e) => {
                // Processing failed. We revert to the original state.
                self.pending_local_requests = tpl_requests.cancel();
                self.pending_remote_requests = tpr_requests.cancel();
                Err(e)
            },
        }
    }
}
