use crypto::identity::PublicKey;
use super::tc_balance::TokenChannelCredit;
use super::invoice_validator::InvoiceValidator;
use super::pending_requests::PendingRequests;
use super::pending_requests::TransPendingRequests;
use super::balance_state_old::RequestSendMessage;
use proto::common::SendFundsReceipt;
use super::balance_state_old::ProcessTransOutput;
use super::balance_state_old::ProcessTransError;
use super::balance_state_old::ResponseSendMessage;
use super::balance_state_old::FailedSendMessage;
use super::balance_state_old::NetworkerTCMessage;
use proto::funder::InvoiceId;


#[derive(Debug)]
pub struct ProcessTransListError {
    index: usize,
    process_trans_error: ProcessTransError,
}

pub struct TokenChannel{
    local_public_key: PublicKey,
    remote_public_key: PublicKey,
    tc_balance: TokenChannelCredit,
    invoice_validator: InvoiceValidator,
    pending_requests: PendingRequests,
}


struct TransTokenChannelState<'a>{
    orig_tc_balance: TokenChannelCredit,
    orig_invoice_validator: InvoiceValidator,

    tc_balance: &'a mut TokenChannelCredit,
    invoice_validator: &'a mut InvoiceValidator,
    trans_pending_requests: TransPendingRequests<'a>,
}

impl TokenChannel{
    pub fn atomic_process_messages_list(&mut self, transactions: Vec<NetworkerTCMessage>)
                                        -> Result<Vec<ProcessTransOutput>, ProcessTransListError>{
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
        TransTokenChannelState{
            orig_tc_balance: token_channel.tc_balance.clone(),
            orig_invoice_validator: token_channel.invoice_validator.clone(),

            tc_balance: &mut token_channel.tc_balance,
            invoice_validator: &mut token_channel.invoice_validator,
            trans_pending_requests: TransPendingRequests::new(&mut token_channel.pending_requests)
        }
    }

    fn process_set_remote_max_debt(&mut self, proposed_max_debt: u64)-> Result<Option<ProcessTransOutput>, ProcessTransError> {
            unreachable!()

    }

    fn process_set_invoice_id(&mut self, invoice_id: InvoiceId)
    -> Result<Option<ProcessTransOutput>, ProcessTransError> {
            unreachable!()
    }

    fn process_load_funds(&mut self, send_funds_receipt: SendFundsReceipt)-> Result<Option<ProcessTransOutput>, ProcessTransError> {
            unreachable!()

    }

    fn process_request_send_message(&mut self,
                                   request_send_msg: RequestSendMessage)-> Result<Option<ProcessTransOutput>, ProcessTransError> {
            unreachable!()

    }


    fn process_response_send_message(&mut self, response_send_msg: ResponseSendMessage)-> Result<Option<ProcessTransOutput>, ProcessTransError> {
            unreachable!()

    }

    fn process_failed_send_message(&mut self, failed_send_msg: FailedSendMessage)-> Result<Option<ProcessTransOutput>, ProcessTransError> {
            unreachable!()

    }

    fn process_message(&mut self, message: NetworkerTCMessage)->
                                        Result<Option<ProcessTransOutput>, ProcessTransError>{
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
    Result<Vec<ProcessTransOutput>, ProcessTransListError>{
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

