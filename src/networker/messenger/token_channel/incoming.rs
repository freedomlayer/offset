use std::convert::TryFrom;

use crypto::identity::{verify_signature};
use crypto::hash;

use proto::funder::InvoiceId;
use proto::common::SendFundsReceipt;
use proto::networker::NetworkerSendPrice;

use utils::int_convert::usize_to_u32;
use utils::safe_arithmetic::SafeArithmetic;

use super::super::types::{ResponseSendMessage, FailureSendMessage, RequestSendMessage,
                                NeighborTcOp, NeighborsRoute, PkPairPosition,
                                PendingNeighborRequest};

use super::super::credit_calc::CreditCalculator;
use super::super::signature_buff::{create_failure_signature_buffer, 
    create_response_signature_buffer};

use super::types::{TokenChannel, TcBalance, TcInvoice, TcSendPrice, TcIdents,
    TransTcPendingRequests, MAX_NETWORKER_DEBT};



/*
pub struct IncomingRequestSendMessage {
    request: RequestSendMessage,
    incoming_response: ResponseSendMessage,
}

pub struct IncomingResponseSendMessage {
    pending_request: PendingNeighborRequest,
    incoming_response: ResponseSendMessage,
}

pub struct IncomingFailedSendMessage {
    pending_request: PendingNeighborRequest,
    incoming_failed: FailedSendMessage,
}
*/

/// Resulting tasks to perform after processing an incoming operation.
pub enum ProcessOperationOutput {
    Request(RequestSendMessage),
    Response(ResponseSendMessage),
    Failure(FailureSendMessage),
}


#[derive(Debug)]
pub enum ProcessOperationError {
    RemoteMaxDebtTooLarge(u64),
    /// Trying to set the invoiceId, while already expecting another invoice id.
    InvoiceIdExists,
    MissingInvoiceId,
    InvalidInvoiceId,
    InvalidSendFundsReceiptSignature,
    PkPairNotInRoute,
    PendingCreditTooLarge,
    /// The Route contains some public key twice.
    DuplicateNodesInRoute,
    LoadFundsOverflow,
    RequestsAlreadyDisabled,
    ResponsePaymentProposalTooLow,
    IncomingRequestsDisabled,
    RoutePricingOverflow,
    RequestContentTooLong,
    RouteTooLong,
    InsufficientTrust,
    InsufficientTransitiveTrust,
    CreditsCalcOverflow,
    IncompatibleFreezeeLinks,
    InvalidFreezeLinks,
    CreditCalculatorFailure,
    RequestAlreadyExists,
    RequestDoesNotExist,
    InvalidResponseSignature,
    ProcessingFeeCollectedTooHigh,
    ResponseContentTooLong,
    ReportingNodeNonexistent,
    InvalidReportingNode,
    InvalidFailureSignature,
}

#[derive(Debug)]
pub struct ProcessTransListError {
    index: usize,
    process_trans_error: ProcessOperationError,
}


/// Processes incoming messages, acts upon an underlying `TokenChannel`.
struct IncomingTokenChannel {
    orig_balance: TcBalance,
    orig_invoice: TcInvoice,
    orig_send_price: TcSendPrice,
    idents: TcIdents,
    balance: TcBalance,
    invoice: TcInvoice,
    send_price: TcSendPrice,
    trans_pending_requests: TransTcPendingRequests,
}

/// If this function returns an error, the token channel becomes incosistent.
pub fn atomic_process_operations_list(token_channel: TokenChannel, operations: Vec<NeighborTcOp>)
                                    -> (TokenChannel, Result<Vec<ProcessOperationOutput>, ProcessTransListError>) {

    let mut trans_token_channel = IncomingTokenChannel::new(token_channel);
    match trans_token_channel.process_operations_list(operations) {
        Err(e) => (trans_token_channel.cancel(), Err(e)),
        Ok(output_tasks) => (trans_token_channel.commit(), Ok(output_tasks)),
    }
}


/*
fn verify_freezing_links(freeze_links: &[NetworkerFreezeLink], 
                            credit_calc: &CreditCalculator) -> Result<(), ProcessOperationError> {

    // Make sure that the freeze_links vector is valid:
    // numerator <= denominator for every link.
    for freeze_link in freeze_links {
        let usable_ratio = &freeze_link.usable_ratio;
        if usable_ratio.numerator > usable_ratio.denominator {
            return Err(ProcessOperationError::InvalidFreezeLinks);
        }
    }

    // Verify previous freezing links
    #[allow(needless_range_loop)]
    for node_findex in 0 .. freeze_links.len() {
        let freeze_link = &freeze_links[node_findex];
        let mut allowed_credits: BigUint = freeze_link.shared_credits.into();
        for fi in node_findex .. freeze_links.len() {
            allowed_credits *= freeze_link.usable_ratio.numerator;
            allowed_credits /= freeze_link.usable_ratio.denominator;
        }

        let freeze_credits = credit_calc.credits_to_freeze(node_findex)
            .ok_or(ProcessOperationError::CreditCalculatorFailure)?;

        if allowed_credits < freeze_credits.into() {
            return Err(ProcessOperationError::InsufficientTransitiveTrust);
        }
    }
    Ok(())
}
*/

/// Verify all failure message signature chain
fn verify_failure_signature(index: usize,
                            reporting_index: usize,
                            failure_send_msg: &FailureSendMessage,
                            pending_request: &PendingNeighborRequest) -> Option<()> {

    let mut failure_signature_buffer = create_failure_signature_buffer(
                                        &failure_send_msg,
                                        &pending_request);
    let next_index = index.checked_add(1)?;
    for i in (next_index ..= reporting_index).rev() {
        let sig_index = i.checked_sub(next_index)?;
        let rand_nonce = &failure_send_msg.rand_nonce_signatures[sig_index].rand_nonce;
        let signature = &failure_send_msg.rand_nonce_signatures[sig_index].signature;
        failure_signature_buffer.extend_from_slice(rand_nonce);
        let public_key = pending_request.route.pk_by_index(i)?;
        if !verify_signature(&failure_signature_buffer, public_key, signature) {
            return None;
        }
    }
    Some(())
}


/// Transactional state of the token channel.
impl IncomingTokenChannel {
    /// original_token_channel: the underlying TokenChannel.
    pub fn new(token_channel: TokenChannel) -> IncomingTokenChannel {
        IncomingTokenChannel {
            orig_balance: token_channel.balance.clone(),
            orig_invoice: token_channel.invoice.clone(),
            orig_send_price: token_channel.send_price.clone(),
            idents: token_channel.idents,
            balance: token_channel.balance,
            invoice: token_channel.invoice,
            send_price: token_channel.send_price,
            trans_pending_requests: TransTcPendingRequests::new(token_channel.pending_requests),
        }
    }

    pub fn cancel(self) -> TokenChannel {
        TokenChannel {
            idents: self.idents,
            balance: self.orig_balance,
            invoice: self.orig_invoice,
            send_price: self.orig_send_price,
            pending_requests: self.trans_pending_requests.cancel(),
        }
    }

    pub fn commit(self) -> TokenChannel {
        TokenChannel {
            idents: self.idents,
            balance: self.balance,
            invoice: self.invoice,
            send_price: self.send_price,
            pending_requests: self.trans_pending_requests.commit(),
        }
    }

    /// Every error is translated into an inconsistency of the token channel.
    pub fn process_operations_list(&mut self, operations: Vec<NeighborTcOp>) ->
        Result<Vec<ProcessOperationOutput>, ProcessTransListError> {
        let mut outputs = Vec::new();

        for (index, message) in operations.into_iter().enumerate() {
            match self.process_operation(message) {
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

    fn process_operation(&mut self, message: NeighborTcOp) ->
        Result<Option<ProcessOperationOutput>, ProcessOperationError> {
        match message {
            NeighborTcOp::EnableRequests(send_price) =>
                self.process_enable_requests(send_price),
            NeighborTcOp::DisableRequests =>
                self.process_disable_requests(),
            NeighborTcOp::SetRemoteMaxDebt(proposed_max_debt) =>
                self.process_set_remote_max_debt(proposed_max_debt),
            NeighborTcOp::SetInvoiceId(rand_nonce) =>
                self.process_set_invoice_id(rand_nonce),
            NeighborTcOp::LoadFunds(send_funds_receipt) =>
                self.process_load_funds(send_funds_receipt),
            NeighborTcOp::RequestSendMessage(request_send_msg) =>
                Ok(Some(ProcessOperationOutput::Request(
                    self.process_request_send_message(request_send_msg)?))),
            NeighborTcOp::ResponseSendMessage(response_send_msg) =>
                Ok(Some(ProcessOperationOutput::Response(
                    self.process_response_send_message(response_send_msg)?))),
            NeighborTcOp::FailureSendMessage(failure_send_msg) =>
                Ok(Some(ProcessOperationOutput::Failure(
                    self.process_failure_send_message(failure_send_msg)?))),
        }
    }

    fn process_enable_requests(&mut self, send_price: NetworkerSendPrice) ->
        Result<Option<ProcessOperationOutput>, ProcessOperationError> {

        self.send_price.remote_send_price = Some(send_price);
        // TODO: Should the price change be reported somewhere?
        Ok(None)
    }

    fn process_disable_requests(&mut self) ->
        Result<Option<ProcessOperationOutput>, ProcessOperationError> {

        self.send_price.remote_send_price = match self.send_price.remote_send_price {
            Some(ref _send_price) => None,
            None => return Err(ProcessOperationError::RequestsAlreadyDisabled),
        };
        // TODO: Should this event be reported somehow?
        Ok(None)
    }

    fn process_set_remote_max_debt(&mut self, proposed_max_debt: u64) -> 
        Result<Option<ProcessOperationOutput>, ProcessOperationError> {

        if proposed_max_debt > MAX_NETWORKER_DEBT {
            Err(ProcessOperationError::RemoteMaxDebtTooLarge(proposed_max_debt))
        } else {
            self.balance.remote_max_debt = proposed_max_debt;
            Ok(None)
        }
    }

    fn process_set_invoice_id(&mut self, invoice_id: InvoiceId)
                              -> Result<Option<ProcessOperationOutput>, ProcessOperationError> {
        if self.invoice.remote_invoice_id.is_none() {
            self.invoice.remote_invoice_id = Some(invoice_id);
            Ok(None)
        } else {
            Err(ProcessOperationError::InvoiceIdExists)
        }
    }

    fn process_load_funds(&mut self, send_funds_receipt: SendFundsReceipt) -> 
        Result<Option<ProcessOperationOutput>, ProcessOperationError> {

        // Check if the SendFundsReceipt is signed properly by us. 
        //      Could we somehow abstract this, for easier testing?
        if !send_funds_receipt.verify_signature(&self.idents.local_public_key) {
            return Err(ProcessOperationError::InvalidSendFundsReceiptSignature);
        }

        // Check if invoice_id matches. If so, we remove the remote invoice.
        match self.invoice.remote_invoice_id.take() {
            None => return Err(ProcessOperationError::MissingInvoiceId),
            Some(invoice_id) => {
                if invoice_id != send_funds_receipt.invoice_id {
                    self.invoice.remote_invoice_id = Some(invoice_id);
                    return Err(ProcessOperationError::InvalidInvoiceId);
                }
            }
        };

        // Add payment to self.balance.balance. We have to be careful because payment u128, and
        // self.balance.balance is of type i64.
        let payment = u64::try_from(send_funds_receipt.payment).unwrap_or(u64::max_value());
        self.balance.balance = self.balance.balance.saturating_add_unsigned(payment);
        Ok(None)
    }

    /// Make sure that we are open to incoming requests, 
    /// and that the offered response proposal is high enough.
    fn verify_local_send_price(&self, 
                               route: &NeighborsRoute, 
                               pk_pair_position: &PkPairPosition) 
        -> Result<(), ProcessOperationError>  {

        let local_send_price = match self.send_price.local_send_price {
            None => Err(ProcessOperationError::IncomingRequestsDisabled),
            Some(ref local_send_price) => Ok(local_send_price.clone()),
        }?;

        let response_proposal = match *pk_pair_position {
            PkPairPosition::Dest => &route.dest_response_proposal,
            PkPairPosition::NotDest(i) => &route.route_links[i].payment_proposal_pair.response,
        };
        // If linear payment proposal for returning response is too low, return error
        if response_proposal.smaller_than(&local_send_price) {
            Err(ProcessOperationError::ResponsePaymentProposalTooLow)
        } else {
            Ok(())
        }
    }

    /// Process an incoming RequestSendMessage
    fn process_request_send_message(&mut self, request_send_msg: RequestSendMessage)
        -> Result<RequestSendMessage, ProcessOperationError> {

        // Make sure that the route does not contains cycles/duplicates:
        if !request_send_msg.route.is_cycle_free() {
            return Err(ProcessOperationError::DuplicateNodesInRoute);
        }

        // Find ourselves on the route. If we are not there, abort.
        let pk_pair = request_send_msg.route.find_pk_pair(
            &self.idents.remote_public_key, 
            &self.idents.local_public_key)
            .ok_or(ProcessOperationError::PkPairNotInRoute)?;

        // Make sure that freeze_links and route_links are compatible in length:
        let freeze_links_len = request_send_msg.freeze_links.len();
        let route_links_len = request_send_msg.route.route_links.len();
        let is_compat = match pk_pair {
            PkPairPosition::Dest => freeze_links_len == route_links_len + 2,
            PkPairPosition::NotDest(i) => freeze_links_len == i
        };
        if !is_compat {
            return Err(ProcessOperationError::InvalidFreezeLinks);
        }

        self.verify_local_send_price(&request_send_msg.route, &pk_pair)?;

        let request_content_len = usize_to_u32(request_send_msg.request_content.len())
            .ok_or(ProcessOperationError::RequestContentTooLong)?;
        let credit_calc = CreditCalculator::new(&request_send_msg.route,
                                                request_content_len,
                                                request_send_msg.processing_fee_proposal,
                                                request_send_msg.max_response_len)
            .ok_or(ProcessOperationError::CreditCalculatorFailure)?;

        // verify_freezing_links(&request_send_msg.freeze_links, &credit_calc)?;

        let index = match pk_pair {
            PkPairPosition::Dest => request_send_msg.route.route_links.len().checked_add(1),
            PkPairPosition::NotDest(i) => i.checked_add(1),
        }.ok_or(ProcessOperationError::RouteTooLong)?;

        // Calculate amount of credits to freeze
        let own_freeze_credits = credit_calc.credits_to_freeze(index)
            .ok_or(ProcessOperationError::CreditCalculatorFailure)?;

        // Make sure we can freeze the credits
        let new_remote_pending_debt = self.balance.remote_pending_debt
            .checked_add(own_freeze_credits).ok_or(ProcessOperationError::CreditsCalcOverflow)?;

        if new_remote_pending_debt > self.balance.remote_max_debt {
            return Err(ProcessOperationError::InsufficientTrust);
        }

        // Note that Verifying our own freezing link will be done outside. We don't have enough
        // information here to check this. In addition, even if it turns out we can't freeze those
        // credits, we don't want to create a token channel inconsistency.         
        
        let p_remote_requests = &mut self.trans_pending_requests.trans_pending_remote_requests;
        // Make sure that we don't have this request as a pending request already:
        if p_remote_requests.get_hmap().contains_key(&request_send_msg.request_id) {
            return Err(ProcessOperationError::RequestAlreadyExists);
        }

        // Add pending request message:
        let request_content_len = usize_to_u32(request_send_msg.request_content.len())
            .ok_or(ProcessOperationError::RequestContentTooLong)?;
        let pending_neighbor_request = PendingNeighborRequest {
            request_id: request_send_msg.request_id,
            route: request_send_msg.route.clone(),
            request_content_hash: hash::sha_512_256(&request_send_msg.request_content),
            request_content_len,
            max_response_len: request_send_msg.max_response_len,
            processing_fee_proposal: request_send_msg.processing_fee_proposal,
        };
        p_remote_requests.insert(request_send_msg.request_id,
                                     pending_neighbor_request);
        
        // If we are here, we can freeze the credits:
        self.balance.remote_pending_debt = new_remote_pending_debt;
        Ok(request_send_msg)
    }

    fn process_response_send_message(&mut self, response_send_msg: ResponseSendMessage) ->
        Result<ResponseSendMessage, ProcessOperationError> {

        // Make sure that id exists in local_pending hashmap, 
        // and access saved request details.
        let local_pending_requests = self.trans_pending_requests
            .trans_pending_local_requests.get_hmap();

        // Obtain pending request:
        let pending_request = local_pending_requests.get(&response_send_msg.request_id)
            .ok_or(ProcessOperationError::RequestDoesNotExist)?;

        let response_signature_buffer = create_response_signature_buffer(
                                            &response_send_msg,
                                            &pending_request);

        // Verify response message signature:
        if !verify_signature(&response_signature_buffer, 
                                 &self.idents.local_public_key,
                                 &response_send_msg.signature) {
            return Err(ProcessOperationError::InvalidResponseSignature);
        }

        // Verify that processing_fee_collected is within range.
        if response_send_msg.processing_fee_collected > pending_request.processing_fee_proposal {
            return Err(ProcessOperationError::ProcessingFeeCollectedTooHigh);
        }

        // Make sure that response_content is not longer than max_response_len.
        let response_content_len = usize_to_u32(response_send_msg.response_content.len())
            .ok_or(ProcessOperationError::ResponseContentTooLong)?;
        if response_content_len > pending_request.max_response_len {
            return Err(ProcessOperationError::ResponseContentTooLong)?;
        }

        let credit_calc = CreditCalculator::new(&pending_request.route,
                                                pending_request.request_content_len,
                                                pending_request.processing_fee_proposal,
                                                pending_request.max_response_len)
            .ok_or(ProcessOperationError::CreditCalculatorFailure)?;

        // Find ourselves on the route. If we are not there, abort.
        let pk_pair = pending_request.route.find_pk_pair(
            &self.idents.remote_public_key, 
            &self.idents.local_public_key)
            .expect("Can not find myself in request's route!");

        let index = match pk_pair {
            PkPairPosition::Dest => pending_request.route.route_links.len().checked_add(1),
            PkPairPosition::NotDest(i) => i.checked_add(1),
        }.expect("Route too long!");

        // Remove entry from local_pending hashmap:
        self.trans_pending_requests.trans_pending_local_requests.remove(
            &response_send_msg.request_id);

        let next_index = index.checked_add(1).expect("Route too long!");
        let success_credits = credit_calc.credits_on_success(next_index, response_content_len)
            .expect("credits_on_success calculation failed!");
        let freeze_credits = credit_calc.credits_to_freeze(next_index)
            .expect("credits_to_freeze calculation failed!");

        // Decrease frozen credits and increase balance:
        self.balance.local_pending_debt = 
            self.balance.local_pending_debt.checked_sub(freeze_credits)
            .expect("Insufficient frozen credit!");

        self.balance.balance = 
            self.balance.balance.checked_sub_unsigned(success_credits)
            .expect("balance overflow");

        Ok(response_send_msg)
    }

    fn process_failure_send_message(&mut self, failure_send_msg: FailureSendMessage) ->
        Result<FailureSendMessage, ProcessOperationError> {
        
        // Make sure that id exists in local_pending hashmap, 
        // and access saved request details.
        let local_pending_requests = self.trans_pending_requests
            .trans_pending_local_requests.get_hmap();

        // Obtain pending request:
        let pending_request = local_pending_requests.get(&failure_send_msg.request_id)
            .ok_or(ProcessOperationError::RequestDoesNotExist)?;

        // Find ourselves on the route. If we are not there, abort.
        let pk_pair = pending_request.route.find_pk_pair(
            &self.idents.remote_public_key, 
            &self.idents.local_public_key)
            .expect("Can not find myself in request's route!");

        let index = match pk_pair {
            PkPairPosition::Dest => pending_request.route.route_links.len().checked_add(1),
            PkPairPosition::NotDest(i) => i.checked_add(1),
        }.expect("Route too long!");


        // Make sure that reporting node public key is:
        //  - inside the route
        //  - After us on the route.
        //  - Not the destination node
        
        let reporting_index = pending_request.route.pk_index(
            &failure_send_msg.reporting_public_key)
            .ok_or(ProcessOperationError::ReportingNodeNonexistent)?;

        let dest_index = pending_request.route.route_links.len()
            .checked_add(1)
            .ok_or(ProcessOperationError::RouteTooLong)?;

        if (reporting_index <= index) || (reporting_index >= dest_index) {
            return Err(ProcessOperationError::InvalidReportingNode);
        }

        verify_failure_signature(index,
                                 reporting_index,
                                 &failure_send_msg,
                                 pending_request)
            .ok_or(ProcessOperationError::InvalidFailureSignature)?;

        // At this point we believe the failure message is valid.

        let credit_calc = CreditCalculator::new(&pending_request.route,
                                                pending_request.request_content_len,
                                                pending_request.processing_fee_proposal,
                                                pending_request.max_response_len)
            .ok_or(ProcessOperationError::CreditCalculatorFailure)?;

        // Remove entry from local_pending hashmap:
        self.trans_pending_requests.trans_pending_local_requests.remove(
            &failure_send_msg.request_id);

        let next_index = index.checked_add(1).expect("Route too long!");
        let failure_credits = credit_calc.credits_on_failure(next_index, reporting_index)
            .expect("credits_on_failure calculation failed!");
        let freeze_credits = credit_calc.credits_to_freeze(next_index)
            .expect("credits_to_freeze calculation failed!");

        // Decrease frozen credits and increase balance:
        self.balance.local_pending_debt = 
            self.balance.local_pending_debt.checked_sub(freeze_credits)
            .expect("Insufficient frozen credit!");

        self.balance.balance = 
            self.balance.balance.checked_sub_unsigned(failure_credits)
            .expect("balance overflow");
        
        // Return Failure message.
        Ok(failure_send_msg)

    }
}
