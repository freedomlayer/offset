use std::mem;
use std::cmp;
use byteorder::{LittleEndian, WriteBytesExt};

use crypto::identity::{PublicKey, verify_signature, Signature};
use crypto::uid::Uid;
use crypto::rand_values::RandValue;
use crypto::hash;

use proto::common::SendFundsReceipt;
use proto::indexer::{NeighborsRoute, PkPairPosition};
use proto::funder::InvoiceId;

use super::tc_credit::TokenChannelCredit;
use super::invoice_validator::InvoiceValidator;
use super::pending_requests::PendingRequests;
use super::pending_requests::TransPendingRequests;
use super::credit_calculator;
use super::super::messages::PendingNeighborRequest;


pub struct IncomingResponseSendMessage {
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
    InvalidFeeProposal,
    PendingCreditTooLarge,
    InvalidResponseSignature,
    /// The Route contains some public key twice.
    DuplicateNodesInRoute,
    LoadFundsOverflow,
}

#[derive(Debug)]
pub struct ProcessTransListError {
    index: usize,
    process_trans_error: ProcessMessageError,
}

pub struct ResponseSendMessage {
    request_id: Uid,
    rand_nonce: RandValue,
    processing_fee_collected: u64,
    response_content: Vec<u8>,
    signature: Signature,
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
        mem::size_of::<Uid>() + 
            mem::size_of::<PublicKey>() * self.route.public_keys.len() +
            self.request_content.len() * 1 +
            mem::size_of_val(&self.max_response_len) +
            mem::size_of_val(&self.processing_fee_proposal) +
            mem::size_of_val(&self.credits_per_byte_proposal)
    }

    pub fn calculate_credits_to_freeze(&self, public_key: &PublicKey) -> Option<u64> {
        let index = self.route.index_of(&public_key);

        credit_calculator::credits_to_freeze(self.processing_fee_proposal,
        self.request_content.len() as u32, self.credits_per_byte_proposal, self.max_response_len,
            self.route.public_keys.len() - 1
        )
    }

    pub fn get_request_id(&self) -> &Uid {
        &self.request_id
    }

    pub fn get_route(&self) -> &NeighborsRoute {
        &self.route
    }

    pub fn create_pending_request(&self) -> PendingNeighborRequest {
        PendingNeighborRequest {
            request_id: self.request_id.clone(),
            route: self.route.clone(),
            request_content_hash: hash::sha_512_256(self.request_content.as_ref()),
            max_response_length: self.max_response_len,
            processing_fee_proposal: self.processing_fee_proposal,
            credits_per_byte_proposal: self.credits_per_byte_proposal,
        }
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
    local_public_key: PublicKey,
    remote_public_key: PublicKey,
    tc_balance: TokenChannelCredit,
    invoice_validator: InvoiceValidator,
    pending_requests: PendingRequests,
}


struct TransTokenChannelState<'a> {
    orig_tc_balance: TokenChannelCredit,
    orig_invoice_validator: InvoiceValidator,
    local_public_key: PublicKey,
    remote_public_key: PublicKey,

    tc_balance: &'a mut TokenChannelCredit,
    invoice_validator: &'a mut InvoiceValidator,
    trans_pending_requests: TransPendingRequests<'a>,
}

impl TokenChannel {
    pub fn atomic_process_messages_list(&mut self, transactions: Vec<NetworkerTCMessage>)
                                        -> Result<Vec<ProcessMessageOutput>, ProcessTransListError>{
        let mut trans_token_channel = TransTokenChannelState::new(self);
        match trans_token_channel.process_messages_list(transactions){
            Err(e) => {
                trans_token_channel.cancel();
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
            trans_pending_requests: TransPendingRequests::new(&mut token_channel.pending_requests)
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
        // Verify signature:
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

    fn process_message_final(&mut self, request_send_msg: RequestSendMessage)-> Result<Option<ProcessMessageOutput>, ProcessMessageError> {
        unreachable!()
    }

    fn process_request_send_message(&mut self, request_send_msg: RequestSendMessage)-> 
        Result<Option<ProcessMessageOutput>, ProcessMessageError> {

        // TODO: Deal with case where we are the the last on the route chain
        if !request_send_msg.route.is_unique(){
            return Err(ProcessMessageError::DuplicateNodesInRoute);
        }

        let credits = match request_send_msg.route.find_pk_pair(&self.remote_public_key, &self.local_public_key) {
            PkPairPosition::NotFound => return Err(ProcessMessageError::PKPairNotInChain),
            PkPairPosition::IsLast => {
                return self.process_request_send_message(request_send_msg);
            },
            PkPairPosition::NotLast => {
                // Make sure it is possible to increase remote_pending_debt, and then increase it.
                request_send_msg.calculate_credits_to_freeze(&self.local_public_key);
            },

        };
        // Check if request_id is not already inside pending_remote_requests.
        // If not, insert into pending_remote_requests.
        if !self.trans_pending_requests.add_pending_remote_request(&request_send_msg){
            return Err(ProcessMessageError::RemoteRequestIdExists);
        }

        // Find myself in the route chain:

        /*
        if !trans_balance_state.credit_state.increase_remote_pending(pending_credit) {
            return (trans_balance_state, Err(ProcessMessageError::PendingCreditTooLarge));
        }
        */
        unreachable!();

        Ok(Some(ProcessMessageOutput::Request(request_send_msg)))

    }


    fn process_response_send_message(&mut self, response_send_msg: ResponseSendMessage) -> 
        Result<Option<ProcessMessageOutput>, ProcessMessageError> {


        // Verify signature
        let mut message = Vec::new();
        message.extend_from_slice(response_send_msg.request_id.as_bytes());
        message.extend_from_slice(response_send_msg.rand_nonce.as_bytes());
        // Serialize the processing_fee_collected:
        message.write_u64::<LittleEndian>(response_send_msg.processing_fee_collected);
        message.extend_from_slice(&response_send_msg.response_content);

        if !verify_signature(&message, &self.remote_public_key, &response_send_msg.signature) {
            return Err(ProcessMessageError::InvalidResponseSignature);
        }


        // Things to do:
        // - Check if we have a pending request that matches this response.
        // - Move credits accordingly (Move balance along pending credits)
        // - Output a task message, telling about a received response message.
        //
        unreachable!()
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
        let mut trans_list_output = Vec::new();

        for (index, message) in messages.into_iter().enumerate() {
            match self.process_message(message){
                Err(e) => return Err(ProcessTransListError {
                    index,
                    process_trans_error: e
                }),
                Ok(Some(trans_output)) => trans_list_output.push(trans_output),
                Ok(None) => {},
            }
        }
        Ok(trans_list_output)
    }

    fn cancel(self){
        *self.tc_balance = self.orig_tc_balance;
        *self.invoice_validator = self.orig_invoice_validator;
        self.trans_pending_requests.cancel();
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
