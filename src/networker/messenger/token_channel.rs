use crypto::identity::PublicKey;
use super::tc_balance::TokenChannelCredit;
use super::invoice_validator::InvoiceValidator;
use super::pending_requests::PendingRequests;
use super::pending_requests::TransPendingRequests;
use super::balance_state_old::RequestSendMessage;
use proto::common::SendFundsReceipt;
use super::balance_state_old::ResponseSendMessage;
use super::balance_state_old::FailedSendMessage;
use super::balance_state_old::NetworkerTCTransaction;
use proto::funder::InvoiceId;



pub struct TokenChannel{
    local_public_key: PublicKey,
    remote_public_key: PublicKey,
    tc_balance: TokenChannelCredit,
    invoice_validator: InvoiceValidator,
    pending_requests: PendingRequests,
}


pub fn atomic_process_trans_list(token_channel: TokenChannel, transactions: Vec<NetworkerTCTransaction>) {

}


struct TransTokenChannelState<'a>{
    local_public_key: PublicKey,
    remote_public_key: PublicKey,
    tc_balance: TokenChannelCredit,
    invoice_validator: InvoiceValidator,
    pending_requests: TransPendingRequests<'a>,
}



impl <'a>TransTokenChannelState<'a>{
    pub fn new(token_channel: &'a mut TokenChannel) -> (){

    }

    fn process_set_remote_max_debt(&mut self, proposed_max_debt: u64){

    }

    fn process_set_invoice_id(&mut self, invoice_id: InvoiceId){

    }

    fn process_load_funds(&mut self, local_public_key: &PublicKey,
                          send_funds_receipt: SendFundsReceipt){

    }

    fn process_request_send_message(&mut self,
                                   request_send_msg: RequestSendMessage){

    }


    fn process_response_send_message(&mut self, response_send_msg: ResponseSendMessage){

    }

    fn process_failed_send_message(&mut self, failed_send_msg: FailedSendMessage){

    }
}

