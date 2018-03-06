use std::cmp;
use std::mem;
use std::collections::HashMap;

use proto::indexer::{NeighborsRoute, PkPairPosition};
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

impl RequestSendMessage {
    pub fn bytes_count(&self) -> usize {
        // We count the bytes count here and not before deserialization,
        // because we actually charge for the amount of bytes we send, and not for the 
        // amount of bytes we receive (Those could possibly be encoded in some strange way)
        mem::size_of::<Uid>() + 
            mem::size_of::<PublicKey>() * self.route.public_keys.len() +
            self.request_content.len() * 1 +
            mem::size_of_val(&self.max_response_len) +
            mem::size_of_val(&self.processing_fee_proposal) +
            mem::size_of_val(&self.credits_per_byte_proposal)
    }
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

impl CreditState {
    fn set_local_max_debt(&mut self, proposed_max_debt: u64) -> bool {
        let max_local_max_debt: u64 = cmp::min(
            MAX_NEIGHBOR_DEBT as i64, 
            (self.local_pending_debt as i64) - self.balance) as u64;

        if proposed_max_debt > max_local_max_debt {
            false
        } else {
            self.local_max_debt = proposed_max_debt;
            true
        }
    }

    fn set_remote_invoice_id(&mut self, invoice_id: InvoiceId) -> bool {
        self.remote_invoice_id = match &self.remote_invoice_id {
            &None => Some(invoice_id),
            &Some(_) => return false,
        };
        true
    }

    fn decrease_balance(&mut self, payment: u128) {
        // Possibly trim payment so that: local_pending_debt - balance < MAX_NEIGHBOR_DEBT
        // This means that the sender of payment is losing some credits in this transaction.
        let max_payment = cmp::max(MAX_NEIGHBOR_DEBT as i64 - 
            self.local_pending_debt as i64 + 
            self.balance as i64, 0) as u64;
        let payment = cmp::min(payment, max_payment as u128) as u64;

        // Apply payment to balance:
        self.balance -= payment as i64;

        // Possibly increase local_max_debt if we got too many credits:
        self.local_max_debt = cmp::max(
            self.local_max_debt as i64, 
            self.local_pending_debt as i64 - self.balance as i64)
                as u64;
    }

    fn increase_remote_pending(&mut self, pending_credit: u64) -> bool {
        if pending_credit as i64 > 
            (self.remote_max_debt as i64) - self.balance 
                - self.remote_max_debt as i64 {
            self.remote_pending_debt += pending_credit;
            true
        } else {
            false
        }
    }
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


pub struct IncomingResponseSendMessage {
}

pub struct IncomingFailedSendMessage {
}

pub enum ProcessTransOutput {
    Request(RequestSendMessage),
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
    PKPairNotInChain,
    RemoteRequestIdExists,
    InvalidFeeProposal,
    PendingCreditTooLarge,
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

    if trans_balance_state.credit_state.set_local_max_debt(proposed_max_debt) {
        (trans_balance_state, Ok(None))
    } else {
        (trans_balance_state, Err(ProcessTransError::RemoteMaxDebtTooLarge(proposed_max_debt)))
    }
}

fn process_set_invoice_id(mut trans_balance_state: TransBalanceState,
                          invoice_id: InvoiceId)
                                    -> (TransBalanceState, 
                                        Result<Option<ProcessTransOutput>, ProcessTransError>) {

    if trans_balance_state.credit_state.set_remote_invoice_id(invoice_id.clone()) {
        (trans_balance_state, Ok(None))
    } else {
        (trans_balance_state, Err(ProcessTransError::InvoiceIdExists))
    }
}


fn process_load_funds(mut trans_balance_state: TransBalanceState,
                      local_public_key: &PublicKey,
                      send_funds_receipt: SendFundsReceipt)
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

    trans_balance_state.credit_state.decrease_balance(send_funds_receipt.payment);

    // Empty local_invoice_id:
    trans_balance_state.credit_state.local_invoice_id = None;

    (trans_balance_state, Ok(None))
}

fn process_request_send_message(mut trans_balance_state: TransBalanceState,
                                    local_public_key: &PublicKey,
                                    remote_public_key: &PublicKey,
                                   request_send_msg: RequestSendMessage)
                                    -> (TransBalanceState, 
                                        Result<Option<ProcessTransOutput>, ProcessTransError>) {

    // TODO: Deal with case where we are the the last on the route chain 
    // (Should get processing fee)
    
    // Check if request_id is not already inside pending_remote_requests.
    // If not, insert into pending_remote_requests.
    let remote_requests = trans_balance_state.tp_remote_requests.get_hmap();
    if remote_requests.contains_key(&request_send_msg.request_id) {
        return (trans_balance_state, Err(ProcessTransError::RemoteRequestIdExists))
    }

    // Find myself in the route chain:
    let pending_credit = match request_send_msg.route.find_pk_pair(remote_public_key, local_public_key) {
        PkPairPosition::NotFound => return (trans_balance_state, Err(ProcessTransError::PKPairNotInChain)),
        PkPairPosition::NotLast => {
            // Make sure it is possible to increase remote_max_debt, and then increase it.
            let per_byte = request_send_msg.credits_per_byte_proposal;
            if per_byte == 0 {
                return (trans_balance_state, Err(ProcessTransError::InvalidFeeProposal))
            }

            // The amount of credit we are expected to freeze if we process this message:
            match per_byte.checked_mul(request_send_msg.bytes_count() as u64) {
                Some(pending_credit) => pending_credit,
                None => return (trans_balance_state, Err(ProcessTransError::PendingCreditTooLarge)),
            }
        },
        PkPairPosition::IsLast => {
            panic!("TODO here");
        },
    };


    if !trans_balance_state.credit_state.increase_remote_pending(pending_credit) {
        return (trans_balance_state, Err(ProcessTransError::PendingCreditTooLarge));
    }

    (trans_balance_state, Ok(Some(ProcessTransOutput::Request(request_send_msg))))
}

fn process_response_send_message(trans_balance_state: TransBalanceState,
                                   response_send_msg: ResponseSendMessage)
                                    -> (TransBalanceState, 
                                        Result<Option<ProcessTransOutput>, ProcessTransError>) {
    unreachable!();
}

fn process_failed_send_message(trans_balance_state: TransBalanceState,
                                   failed_send_msg: FailedSendMessage)
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
                 trans: NetworkerTCTransaction)
                    -> (TransBalanceState, 
                        Result<Option<ProcessTransOutput>, ProcessTransError>) {

    match trans {
        NetworkerTCTransaction::SetRemoteMaxDebt(proposed_max_debt) => 
            process_set_remote_max_debt(trans_balance_state,
                                        proposed_max_debt),
        NetworkerTCTransaction::SetInvoiceId(rand_nonce) =>
            process_set_invoice_id(trans_balance_state,
                                     rand_nonce),
        NetworkerTCTransaction::LoadFunds(send_funds_receipt) => 
            process_load_funds(trans_balance_state,
                               local_public_key,
                               send_funds_receipt),
        NetworkerTCTransaction::RequestSendMessage(request_send_msg) =>
            process_request_send_message(trans_balance_state,
                                         local_public_key,
                                         remote_public_key,
                                         request_send_msg),
        NetworkerTCTransaction::ResponseSendMessage(response_send_msg) =>
            process_response_send_message(trans_balance_state,
                                          response_send_msg),
        NetworkerTCTransaction::FailedSendMessage(failed_send_msg) => 
            process_failed_send_message(trans_balance_state,
                                        failed_send_msg),
    }
}

fn process_trans_list(mut trans_balance_state: TransBalanceState, 
                      local_public_key: &PublicKey,
                      remote_public_key: &PublicKey,
                      transactions: Vec<NetworkerTCTransaction>)
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
                                 transactions: Vec<NetworkerTCTransaction>)
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


#[cfg(test)]
mod tests {
    use super::*;
    use ring::test::rand::FixedByteRandom;
    use crypto::uid::Uid;


    #[test]
    fn test_request_send_msg_bytes_count() {
        let rng1 = FixedByteRandom { byte: 0x03 };

        assert_eq!(mem::size_of::<PublicKey>(), 32);

        let rsm = RequestSendMessage {
            request_id: Uid::new(&rng1),
            route: NeighborsRoute {
                public_keys: vec![
                    PublicKey::from_bytes(&vec![0u8; 32]).unwrap(),
                    PublicKey::from_bytes(&vec![0u8; 32]).unwrap(),
                ],
            },
            request_content: vec![1,2,3,4,5],
            max_response_len: 0x200,
            processing_fee_proposal: 1,
            credits_per_byte_proposal: 2,
        };

        let expected = 16 + 32 + 32 + 5 + 4 + 8 + 8;
        assert_eq!(rsm.bytes_count(), expected);
    }
}
