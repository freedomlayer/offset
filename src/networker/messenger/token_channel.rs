use std::mem;
use std::cmp;
use byteorder::{LittleEndian, WriteBytesExt};

use crypto::identity::{PublicKey, verify_signature, Signature};
use crypto::uid::Uid;
use crypto::rand_values::RandValue;
use crypto::hash;
use crypto::hash::HashResult;

use proto::common::SendFundsReceipt;
use proto::indexer::{NeighborsRoute, PkPairPosition};
use proto::funder::InvoiceId;

use super::tc_credit::TokenChannelCredit;
use super::invoice_validator::InvoiceValidator;
use super::pending_requests::PendingRequests;
use super::pending_requests::TransPendingRequests;
use super::credit_calculator;
use super::pending_neighbor_request::PendingNeighborRequest;
use super::messenger_messages::{ResponseSendMessage, FailedSendMessage, RequestSendMessage};
use utils::convert_int;
use super::messenger_messages::NetworkerTCMessage;

pub struct IncomingResponseSendMessage {
    pending_request: PendingNeighborRequest,
    incoming_response: ResponseSendMessage,
}

pub struct IncomingFailedSendMessage {
    pending_request: PendingNeighborRequest,
    incoming_failed: FailedSendMessage,
}


/// Resulting tasks to perform after processing an incoming message.
/// Note that
pub enum ProcessMessageOutput {
    Request(RequestSendMessage),
    Response(IncomingResponseSendMessage),
    Failure(IncomingFailedSendMessage),
}


#[derive(Debug)]
pub enum ProcessMessageError {
    RemoteMaxDebtTooLarge(u64),
    /// Trying to set the invoiceId, while already expecting another invoice id.
    InvoiceIdExists,
    MissingInvoiceId,
    InvalidInvoiceId,
    InvalidFundsReceipt,
    PKPairNotInChain,
    RemoteRequestIdExists,
    RequestIdNotExists,
    InvalidFeeProposal,
    PendingCreditTooLarge,
    InvalidResponseSignature,
    /// The Route contains some public key twice.
    DuplicateNodesInRoute,
    LoadFundsOverflow,
    CreditsCalculationOverflow,
    TooLongMessage,
    TooMuchFeeCollected,
    InvalidFailedSignature,
    InvalidFailureReporter,
    InnerBug,
}

#[derive(Debug)]
pub struct ProcessTransListError {
    index: usize,
    process_trans_error: ProcessMessageError,
}




pub struct TokenChannel {
    /// My public key
    local_public_key: PublicKey,
    /// Neighbor's public key
    remote_public_key: PublicKey,
    /// The balance - how much do we owe each other, and what are the limits for this debt.
    tc_balance: TokenChannelCredit,
    /// Validates an incoming invoice
    invoice_validator: InvoiceValidator,
    /// All pending requests - both incoming and outgoing
    pending_requests: PendingRequests,
}


/// Processes incoming messages, acts upon an underlying `TokenChannel`.
struct TransTokenChannelState<'a> {
    /// The original balance
    orig_tc_balance: TokenChannelCredit,
    /// The original invoice
    orig_invoice_validator: InvoiceValidator,
    local_public_key: PublicKey,
    remote_public_key: PublicKey,

    /// Pointer to the balance of the underlying TokenChannel
    tc_balance: &'a mut TokenChannelCredit,
    /// Pointer to the invoice validator of the underlying TokenChannel
    invoice_validator: &'a mut InvoiceValidator,
    // Pointers to the pending requests of the underlying TokenChannel.
    //  transactional_local_pending_requests, transactional_remote_pending_requests
    // together form TokenChannel.pending_requests
    transactional_local_pending_requests: TransPendingRequests<'a>,
    transactional_remote_pending_requests: TransPendingRequests<'a>,
}

/// Processes transactions - list of incoming messages.
impl TokenChannel {
    /// If this function returns an error, the token channel becomes incosistent.
    pub fn atomic_process_messages_list(&mut self, messages: Vec<NetworkerTCMessage>)
                                        -> Result<Vec<ProcessMessageOutput>, ProcessTransListError>{
        let mut transactional_token_channel = TransTokenChannelState::new(self);
        match transactional_token_channel.process_messages_list(messages){
            Err(e) => {
                transactional_token_channel.cancel();
                Err(e)
            },
            Ok(output_tasks) =>{
                Ok(output_tasks)
            }
        }
    }

    pub fn get_remote_max_debt(&self) -> u64{
        self.tc_balance.get_remote_max_debt()
    }

    pub fn get_local_max_debt(&self) -> u64{
        self.tc_balance.get_local_max_debt()
    }
}

/// Transactional state of the token channel.
/// Call cancel() to abort all changes to the channel, do nothing in order to apply the changes
/// on the underlying `TokenChannel`.

impl <'a>TransTokenChannelState<'a>{
    /// original_token_channel: the underlying TokenChannel.
    pub fn new(original_token_channel: &'a mut TokenChannel) -> TransTokenChannelState<'a> {
        let (local_requests, remote_requests) =
            TransPendingRequests::new_transactionals(&mut original_token_channel.pending_requests);
        TransTokenChannelState {
            orig_tc_balance: original_token_channel.tc_balance.clone(),
            orig_invoice_validator: original_token_channel.invoice_validator.clone(),

            remote_public_key: original_token_channel.remote_public_key.clone(),
            local_public_key: original_token_channel.local_public_key.clone(),

            tc_balance: &mut original_token_channel.tc_balance,
            invoice_validator: &mut original_token_channel.invoice_validator,

            transactional_local_pending_requests: local_requests,
            transactional_remote_pending_requests: remote_requests,
        }
    }

    fn process_set_remote_max_debt(&mut self, proposed_max_debt: u64)-> Result<Option<ProcessMessageOutput>, ProcessMessageError> {
        if self.tc_balance.set_remote_max_debt(proposed_max_debt) {
            Ok(None)
        }else{
            Err(ProcessMessageError::RemoteMaxDebtTooLarge(proposed_max_debt))
        }
    }

    fn process_set_invoice_id(&mut self, invoice_id: InvoiceId)
                              -> Result<Option<ProcessMessageOutput>, ProcessMessageError> {
        if self.invoice_validator.set_remote_invoice_id(invoice_id.clone()) {
            Ok(None)
        }else{
            Err(ProcessMessageError::InvoiceIdExists)
        }
    }

    fn process_load_funds(&mut self, send_funds_receipt: SendFundsReceipt)-> Result<Option<ProcessMessageOutput>, ProcessMessageError> {
        match self.invoice_validator.validate_receipt(&send_funds_receipt,
                                                      &self.local_public_key){
            Ok(()) => {
                // TODO(a4vision): The actual payment redeemed for networking cannot be u128.
                //                  Solution: truncate to i64.
                if self.tc_balance.decrease_balance(send_funds_receipt.payment) {
                    Ok(None)
                }else{
                    Err(ProcessMessageError::LoadFundsOverflow)
                }
            },
            Err(e) => Err(e),
        }
    }

    fn process_request_message_last_node(&mut self, request_send_msg: RequestSendMessage)
        -> Result<Option<ProcessMessageOutput>, ProcessMessageError> {
        let credits = request_send_msg.credits_to_freeze_on_destination().
            ok_or(ProcessMessageError::CreditsCalculationOverflow)?;
        if !self.tc_balance.freeze_remote_credits(credits){
            return Err(ProcessMessageError::PendingCreditTooLarge);
        }

        Ok(Some(ProcessMessageOutput::Request(request_send_msg)))
    }

    fn process_request_send_message(&mut self, request_send_msg: RequestSendMessage)->
    Result<Option<ProcessMessageOutput>, ProcessMessageError> {
        if !request_send_msg.get_route().is_unique(){
            return Err(ProcessMessageError::DuplicateNodesInRoute);
        }

        let pending_request = match request_send_msg.get_route().find_pk_pair(&self.remote_public_key, &self.local_public_key) {
            PkPairPosition::NotFound => return Err(ProcessMessageError::PKPairNotInChain),
            PkPairPosition::IsLast => {
                return self.process_request_message_last_node(request_send_msg);
            },
            PkPairPosition::NotLast => {
                request_send_msg.create_pending_request(&self.remote_public_key).ok_or(ProcessMessageError::TooLongMessage)?
            },
        };
        let credits_to_freeze = pending_request.credits_to_freeze().ok_or(
            ProcessMessageError::CreditsCalculationOverflow)?;

        if !self.transactional_remote_pending_requests.add_pending_request(pending_request) {
            return Err(ProcessMessageError::RemoteRequestIdExists);
        }
        if !self.tc_balance.freeze_remote_credits(credits_to_freeze){
            return Err(ProcessMessageError::PendingCreditTooLarge);
        }
        Ok(Some(ProcessMessageOutput::Request(request_send_msg)))
    }


    fn remove_local_pending_request(&mut self, request_id: &Uid) -> Result<PendingNeighborRequest, ProcessMessageError>{
        self.transactional_local_pending_requests.remove_pending_request(request_id).
            ok_or(ProcessMessageError::RequestIdNotExists)
    }

    fn rebalance_credits_upon_receive_response(&mut self, pending_request: &PendingNeighborRequest,
                                               response_send_msg: &ResponseSendMessage)
                                               -> Result<(), ProcessMessageError> {
        let credits_to_realize = pending_request.credits_on_success(response_send_msg.response_length()).
            ok_or(ProcessMessageError::CreditsCalculationOverflow)?;

        self.realize_some_of_frozen_credits(pending_request, credits_to_realize)
    }



    fn process_response_send_message(&mut self, response_send_msg: ResponseSendMessage) ->
    Result<Option<ProcessMessageOutput>, ProcessMessageError> {
        let pending_request = self.remove_local_pending_request(response_send_msg.get_request_id())?;
        pending_request.verify_response_message(&response_send_msg)?;
        self.rebalance_credits_upon_receive_response(&pending_request, &response_send_msg)?;

        Ok(Some(ProcessMessageOutput::Response(IncomingResponseSendMessage{
            pending_request, incoming_response: response_send_msg
        })))
    }

    fn realize_some_of_frozen_credits(&mut self, pending_request: &PendingNeighborRequest,
    credits_to_realize: u64) -> Result<(), ProcessMessageError>{
        let frozen_credits = pending_request.credits_to_freeze().ok_or(ProcessMessageError::CreditsCalculationOverflow)?;

        if !self.tc_balance.realize_local_frozen_credits(credits_to_realize){
            return Err(ProcessMessageError::InnerBug);
        }

        let credits_to_unfreeze = frozen_credits.checked_sub(credits_to_realize).
            ok_or(ProcessMessageError::InnerBug)?;
        if !self.tc_balance.unfreeze_local_credits(credits_to_unfreeze){
            return Err(ProcessMessageError::InnerBug);
        }
        Ok(())
    }

    fn rebalance_credits_upon_receive_failed(&mut self, pending_request: &PendingNeighborRequest,
            failed_send_message: &FailedSendMessage) -> Result<(), ProcessMessageError>{

        let nodes_to_reporting = failed_send_message.nodes_to_reporting(&self.local_public_key,
                                                                        pending_request.get_route()).
            ok_or(ProcessMessageError::InnerBug)?;
        let credits_to_realize = pending_request.credits_on_failure(nodes_to_reporting).
            ok_or(ProcessMessageError::CreditsCalculationOverflow)?;

        self.realize_some_of_frozen_credits(pending_request, credits_to_realize)
    }

    fn process_failed_send_message(&mut self, failed_send_msg: FailedSendMessage) ->
    Result<Option<ProcessMessageOutput>, ProcessMessageError> {
        let pending_request = self.remove_local_pending_request(failed_send_msg.get_request_id())?;
        pending_request.verify_failed_message(&self.local_public_key, &failed_send_msg)?;
        self.rebalance_credits_upon_receive_failed(&pending_request, &failed_send_msg)?;

        Ok(Some(ProcessMessageOutput::Failure(IncomingFailedSendMessage{
            pending_request, incoming_failed: failed_send_msg
        })))
    }

    fn process_message(&mut self, message: NetworkerTCMessage)->
    Result<Option<ProcessMessageOutput>, ProcessMessageError>{
        match message {
            NetworkerTCMessage::SetRemoteMaxDebt(proposed_max_debt) =>
                self.process_set_remote_max_debt(proposed_max_debt),
            NetworkerTCMessage::SetInvoiceId(rand_nonce) =>
                self.process_set_invoice_id(rand_nonce),
            NetworkerTCMessage::LoadFunds(send_funds_receipt) =>
                self.process_load_funds(send_funds_receipt),
            NetworkerTCMessage::RequestSendMessage(request_send_msg) =>
                self.process_request_send_message(request_send_msg),
            NetworkerTCMessage::ResponseSendMessage(response_send_msg) =>
                self.process_response_send_message(response_send_msg),
            NetworkerTCMessage::FailedSendMessage(failed_send_msg) =>
                self.process_failed_send_message(failed_send_msg),
        }
    }

    /// Every error is translated into an inconsistency of the token channel.
    pub fn process_messages_list(&mut self, messages: Vec<NetworkerTCMessage>) ->
    Result<Vec<ProcessMessageOutput>, ProcessTransListError>{
        let mut outputs = Vec::new();

        for (index, message) in messages.into_iter().enumerate() {
            match self.process_message(message){
                Err(e) => return Err(ProcessTransListError {
                    index,
                    process_trans_error: e
                }),
                Ok(Some(trans_output)) => outputs.push(trans_output),
                Ok(None) => {},
            }
        }
        Ok(outputs)
    }

    pub fn cancel(self){
        *self.tc_balance = self.orig_tc_balance;
        *self.invoice_validator = self.orig_invoice_validator;
        self.transactional_local_pending_requests.cancel();
        self.transactional_remote_pending_requests.cancel();
    }
}

