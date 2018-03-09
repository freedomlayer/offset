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
use super::super::messages::PendingNeighborRequest;
use utils::convert_int;

pub struct IncomingResponseSendMessage {
    request: PendingNeighborRequest,
    response: ResponseSendMessage,

}

pub struct IncomingFailedSendMessage {
}

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
}

#[derive(Debug)]
pub struct ProcessTransListError {
    index: usize,
    process_trans_error: ProcessMessageError,
}

pub struct ResponseSendMessage {
    request_id: Uid,
    // TODO(a4vision): Discuss it: What is this nonce for ?
    rand_nonce: RandValue,
    // TODO(a4vision): Discuss it: Is it possible that the final destination will accept a processing
    //                  fee lower than the assigned processing fee ?
    processing_fee_collected: u64,
    response_content: Vec<u8>,
    signature: Signature,
}

impl ResponseSendMessage{
    pub fn verify_signature(&self, public_key: &PublicKey, request_hash: &HashResult) -> bool{
        let mut message = Vec::new();
        message.extend_from_slice(self.request_id.as_bytes());
        message.extend_from_slice(self.rand_nonce.as_bytes());
        // Serialize the processing_fee_collected:
        message.write_u64::<LittleEndian>(self.processing_fee_collected);
        message.extend_from_slice(&self.response_content);
        // TODO(a4vision): Discuss it.
        message.extend(request_hash.as_ref());
        return verify_signature(&message, &public_key, &self.signature);
    }

    pub fn bytes_count(&self) -> usize{
        // TODO(a4vision): Should we check for an overflow here ?
        mem::size_of_val(&self.request_id) +
        mem::size_of_val(&self.rand_nonce) +
        mem::size_of_val(&self.processing_fee_collected) +
        self.response_content.len() * 1 +
        mem::size_of_val(&self.signature)
    }
}

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

        // TODO(a4vision): Should we check for an overflow here ?
        mem::size_of::<Uid>() +
            mem::size_of::<PublicKey>() * self.route.public_keys.len() +
            self.request_content.len() * 1 +
            mem::size_of_val(&self.max_response_len) +
            mem::size_of_val(&self.processing_fee_proposal) +
            mem::size_of_val(&self.credits_per_byte_proposal)
    }

    fn nodes_to_dest(&self, public_key: &PublicKey) -> Option<usize> {
        let index = self.route.index_of(&public_key)?;
        Some(self.route.public_keys.len() - index - 1)
    }

    pub fn get_request_id(&self) -> &Uid {
        &self.request_id
    }

    pub fn create_pending_request(&self, public_key: &PublicKey) -> Option<PendingNeighborRequest> {
        Some(PendingNeighborRequest {
            request_id: self.request_id.clone(),
            route: self.route.clone(),
            // TODO(a4vision): Discuss it: Shouldn't this hash be over the whole request ??? Otherwise,
            //                  some mediators might change parameters along the way.
            request_content_len: convert_int::checked_as_u32(self.request_content.len())?,
            request_content_hash: hash::sha_512_256(self.request_content.as_ref()),
            max_response_length: self.max_response_len,
            processing_fee_proposal: self.processing_fee_proposal,
            credits_per_byte_proposal: self.credits_per_byte_proposal,
            nodes_to_dest: self.nodes_to_dest(public_key)?,
        })
    }
}


pub struct FailedSendMessage {
    request_id: Uid,
    reporting_public_key: PublicKey,
    rand_nonce: RandValue,
    signature: Signature,
}


pub enum NetworkerTCMessage {
    SetRemoteMaxDebt(u64),
    SetInvoiceId(InvoiceId),
    LoadFunds(SendFundsReceipt),
    RequestSendMessage(RequestSendMessage),
    ResponseSendMessage(ResponseSendMessage),
    FailedSendMessage(FailedSendMessage),
    // ResetChannel(i64), // new_balanace
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


struct TransTokenChannelState<'a> {
    orig_tc_balance: TokenChannelCredit,
    orig_invoice_validator: InvoiceValidator,
    local_public_key: PublicKey,
    remote_public_key: PublicKey,

    tc_balance: &'a mut TokenChannelCredit,
    invoice_validator: &'a mut InvoiceValidator,
    transactional_pending_requests: TransPendingRequests<'a>,
}

impl TokenChannel {
    // TODO(a4vision): Discuss it: Shouldn't we charge the Failure credit for every incoming message ?
    //                  When the function process_request_send_message fails, we need to
    //                  charge for it
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
}

impl <'a>TransTokenChannelState<'a>{
    pub fn new(token_channel: &'a mut TokenChannel) -> TransTokenChannelState<'a> {
        TransTokenChannelState {
            orig_tc_balance: token_channel.tc_balance.clone(),
            orig_invoice_validator: token_channel.invoice_validator.clone(),

            remote_public_key: token_channel.remote_public_key.clone(),
            local_public_key: token_channel.local_public_key.clone(),

            tc_balance: &mut token_channel.tc_balance,
            invoice_validator: &mut token_channel.invoice_validator,
            transactional_pending_requests: TransPendingRequests::new(&mut token_channel.pending_requests)
        }
    }

    fn process_set_remote_max_debt(&mut self, proposed_max_debt: u64)-> Result<Option<ProcessMessageOutput>, ProcessMessageError> {
        match self.tc_balance.set_remote_max_debt(proposed_max_debt) {
            true => Ok(None),
            false => Err(ProcessMessageError::RemoteMaxDebtTooLarge(proposed_max_debt)),
        }
    }

    fn process_set_invoice_id(&mut self, invoice_id: InvoiceId)
                              -> Result<Option<ProcessMessageOutput>, ProcessMessageError> {
        // TODO(a4vision): What if we set the invoice id, and then regret about it ? One cannot reset it.
        match self.invoice_validator.set_remote_invoice_id(invoice_id.clone()) {
            true=> Ok(None),
            false=> Err(ProcessMessageError::InvoiceIdExists),
        }
    }

    fn process_load_funds(&mut self, send_funds_receipt: SendFundsReceipt)-> Result<Option<ProcessMessageOutput>, ProcessMessageError> {
        match self.invoice_validator.validate_receipt(&send_funds_receipt,
                                                      &self.local_public_key){
            Ok(()) => {
                // TODO(a4vision): The actual payment redeemed for networking cannot be u128.
                match self.tc_balance.decrease_balance(send_funds_receipt.payment) {
                    true => Ok(None),
                    false => Err(ProcessMessageError::LoadFundsOverflow),
                }
            },
            Err(e) => return Err(e),
        }
    }

    fn process_request_message_last_node(&mut self, request_send_msg: RequestSendMessage) -> Result<Option<ProcessMessageOutput>, ProcessMessageError> {
        // TODO(a4vision): implement
        unreachable!()
    }

    fn process_request_send_message(&mut self, request_send_msg: RequestSendMessage)->
    Result<Option<ProcessMessageOutput>, ProcessMessageError> {
        // TODO(a4vision): Somewhere here, we should create some failure message in case of an
        //                  invalid RequestSendMessage
        if !request_send_msg.route.is_unique(){
            return Err(ProcessMessageError::DuplicateNodesInRoute);
        }

        let pending_request = match request_send_msg.route.find_pk_pair(&self.remote_public_key, &self.local_public_key) {
            PkPairPosition::NotFound => return Err(ProcessMessageError::PKPairNotInChain),
            PkPairPosition::IsLast => {
                return self.process_request_message_last_node(request_send_msg);
            },
            PkPairPosition::NotLast => {
                request_send_msg.create_pending_request(&self.local_public_key)
            },
        };
        match pending_request{
            None => {
                return Err(ProcessMessageError::TooLongMessage);
            },
            Some(pending_request) => {
                match pending_request.credits_to_freeze(){
                    None => {
                        return Err(ProcessMessageError::CreditsCalculationOverflow);
                    },
                    Some(credits) =>{
                        if !self.transactional_pending_requests.add_pending_remote_request(request_send_msg.get_request_id().clone(),
                                                                                           pending_request) {
                            return Err(ProcessMessageError::RemoteRequestIdExists);
                        }
                        if !self.tc_balance.freeze_remote_credits(credits){
                            return Err(ProcessMessageError::PendingCreditTooLarge);
                        }
                        return Ok(Some(ProcessMessageOutput::Request(request_send_msg)));
                    }
                }
            }
        }
    }


    fn process_response_send_message(&mut self, response_send_msg: ResponseSendMessage) ->
    Result<Option<ProcessMessageOutput>, ProcessMessageError> {
        let pending_request_option = self.transactional_pending_requests.remove_local_pending_request(&response_send_msg.request_id);
        let pending_request = match pending_request_option{
            None => {return Err(ProcessMessageError::RequestIdNotExists);},
            Some(request) => request,
        };

        let destionation_key_option = pending_request.route.destination_public_key();
        let destination_key = match destionation_key_option{
            None => {
                // TODO(a4vision): What is the correct way to handle this case ?
                unreachable!()},
            Some(key) => key,
        };

        if !response_send_msg.verify_signature(&destination_key, &pending_request.request_content_hash){
            return Err(ProcessMessageError::InvalidResponseSignature);
        }
        // TODO(a4vision): Did we mean bytes_count, or response_send_msg.response_content.len() ?
        let response_length = match convert_int::checked_as_u32(response_send_msg.bytes_count()){
            None => return Err(ProcessMessageError::TooLongMessage),
            Some(length) => length,
        };

        if response_length > pending_request.max_response_length{
            return Err(ProcessMessageError::TooLongMessage);
        }

        if pending_request.processing_fee_proposal < response_send_msg.processing_fee_collected{
            return Err(ProcessMessageError::TooMuchFeeCollected);
        }

        let frozen_credits = match pending_request.credits_to_freeze(){
            None => return Err(ProcessMessageError::CreditsCalculationOverflow),
            Some(credits) => credits,

        };

        let credits_to_realize = match pending_request.credits_on_success(response_length as usize){
            None => return Err(ProcessMessageError::CreditsCalculationOverflow),
            Some(credits) => credits,
        };

        if !self.tc_balance.realize_local_frozen_credits(credits_to_realize){
            // TODO(a4vision): What is the correct way to handle this case ?
            unreachable!()
        }
        // Should we check for an overflow in this substraction ?
        if !self.tc_balance.unfreeze_local_credits(frozen_credits - credits_to_realize){
            // TODO(a4vision): What is the correct way to handle this case ?
            unreachable!()
        }

        return Ok(Some(ProcessMessageOutput::Response(IncomingResponseSendMessage{
            request: pending_request, response: response_send_msg
        })));
    }

    fn process_failed_send_message(&mut self, failed_send_msg: FailedSendMessage) ->
    Result<Option<ProcessMessageOutput>, ProcessMessageError> {

        unreachable!()
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

    fn process_messages_list(&mut self, messages: Vec<NetworkerTCMessage>) ->
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

    fn cancel(self){
        *self.tc_balance = self.orig_tc_balance;
        *self.invoice_validator = self.orig_invoice_validator;
        self.transactional_pending_requests.cancel();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem;
    use ring::test::rand::FixedByteRandom;
    use crypto::uid::Uid;
    use proto::indexer::NeighborsRoute;


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
