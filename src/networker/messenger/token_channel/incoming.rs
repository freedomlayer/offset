use std::convert::TryFrom;

use crypto::identity::{verify_signature};

use proto::funder::InvoiceId;
use proto::common::SendFundsReceipt;
use proto::networker::NetworkerSendPrice;

use utils::int_convert::usize_to_u32;
use utils::safe_arithmetic::SafeArithmetic;

use super::super::types::{ResponseSendMessage, FailureSendMessage, RequestSendMessage,
                          NeighborTcOp, NeighborsRoute, PkPairPosition,
                          PendingNeighborRequest };

use super::super::credit_calc::CreditCalculator;
use super::super::signature_buff::{create_response_signature_buffer, verify_failure_signature};

use super::types::{TokenChannel, MAX_NETWORKER_DEBT, TcMutation};


pub struct IncomingRequestSendMessage {
    pub request: PendingNeighborRequest,
}

pub struct IncomingResponseSendMessage {
    pub pending_request: PendingNeighborRequest,
    pub incoming_response: ResponseSendMessage,
}

pub struct IncomingFailureSendMessage {
    pub pending_request: PendingNeighborRequest,
    pub incoming_failure: FailureSendMessage,
}

pub enum IncomingMessage {
    Request(RequestSendMessage),
    Response(IncomingResponseSendMessage),
    Failure(IncomingFailureSendMessage),
}

/// Resulting tasks to perform after processing an incoming operation.
#[allow(unused)]
pub struct ProcessOperationOutput {
    pub incoming_message: Option<IncomingMessage>,
    pub tc_mutations: Vec<TcMutation>,
}


#[derive(Debug)]
pub enum ProcessOperationError {
    RemoteMaxDebtTooLarge(u64),
    /// Trying to set the invoiceId, while already expecting another invoice id.
    InvoiceIdExists,
    MissingInvoiceId,
    InvoiceIdMismatch,
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


pub fn simulate_process_operations_list(token_channel: &TokenChannel, 
                                        operations: Vec<NeighborTcOp>) ->
    Result<Vec<ProcessOperationOutput>, ProcessTransListError> {

    let mut outputs = Vec::new();

    // We do not change the original TokenChannel. 
    // Instead, we are operating over a clone:
    // This operation is not very expensive, because we are using immutable data structures
    // (specifically, HashMaps).
    let mut cloned_token_channel = token_channel.clone();

    for (index, message) in operations.into_iter().enumerate() {
        match process_operation(&mut cloned_token_channel, message) {
            Err(e) => return Err(ProcessTransListError {
                index,
                process_trans_error: e
            }),
            Ok(trans_output) => outputs.push(trans_output),
        }
    }
    Ok(outputs)
}

fn process_operation(token_channel: &mut TokenChannel, message: NeighborTcOp) ->
    Result<ProcessOperationOutput, ProcessOperationError> {
    match message {
        NeighborTcOp::EnableRequests(send_price) =>
            process_enable_requests(token_channel, send_price),
        NeighborTcOp::DisableRequests =>
            process_disable_requests(token_channel),
        NeighborTcOp::SetRemoteMaxDebt(proposed_max_debt) =>
            process_set_remote_max_debt(token_channel, proposed_max_debt),
        NeighborTcOp::SetInvoiceId(rand_nonce) =>
            process_set_invoice_id(token_channel, rand_nonce),
        NeighborTcOp::LoadFunds(send_funds_receipt) =>
            process_load_funds(token_channel, send_funds_receipt),
        NeighborTcOp::RequestSendMessage(request_send_msg) =>
            process_request_send_message(token_channel, request_send_msg),
        NeighborTcOp::ResponseSendMessage(response_send_msg) =>
            process_response_send_message(token_channel, response_send_msg),
        NeighborTcOp::FailureSendMessage(failure_send_msg) =>
            process_failure_send_message(token_channel, failure_send_msg),
    }
}

fn process_enable_requests(token_channel: &mut TokenChannel, 
                           send_price: NetworkerSendPrice) ->
    Result<ProcessOperationOutput, ProcessOperationError> {

    let mut op_output = ProcessOperationOutput {
        incoming_message: None,
        tc_mutations: Vec::new(),
    };
    let tc_mutation = TcMutation::SetRemoteSendPrice(send_price);
    token_channel.mutate(&tc_mutation);
    op_output.tc_mutations.push(tc_mutation);

    Ok(op_output)
}

fn process_disable_requests(token_channel: &mut TokenChannel) ->
    Result<ProcessOperationOutput, ProcessOperationError> {

    let mut op_output = ProcessOperationOutput {
        incoming_message: None,
        tc_mutations: Vec::new(),
    };

    if token_channel.state().send_price.remote_send_price.is_none() {
        Err(ProcessOperationError::RequestsAlreadyDisabled)
    } else {
        let tc_mutation = TcMutation::ClearRemoteSendPrice;
        token_channel.mutate(&tc_mutation);
        op_output.tc_mutations.push(tc_mutation);
        Ok(op_output)
    }
}

fn process_set_remote_max_debt(token_channel: &mut TokenChannel,
                               proposed_max_debt: u64) -> 
    Result<ProcessOperationOutput, ProcessOperationError> {

    let mut op_output = ProcessOperationOutput {
        incoming_message: None,
        tc_mutations: Vec::new(),
    };

    if proposed_max_debt > MAX_NETWORKER_DEBT {
        Err(ProcessOperationError::RemoteMaxDebtTooLarge(proposed_max_debt))
    } else {
        let tc_mutation = TcMutation::SetLocalMaxDebt(proposed_max_debt);
        token_channel.mutate(&tc_mutation);
        op_output.tc_mutations.push(tc_mutation);
        Ok(op_output)
    }
}

fn process_set_invoice_id(token_channel: &mut TokenChannel,
                          invoice_id: InvoiceId)
                          -> Result<ProcessOperationOutput, ProcessOperationError> {
    let mut op_output = ProcessOperationOutput {
        incoming_message: None,
        tc_mutations: Vec::new(),
    };

    if token_channel.state().invoice.remote_invoice_id.is_none() {
        let tc_mutation = TcMutation::SetRemoteInvoiceId(invoice_id.clone());
        token_channel.mutate(&tc_mutation);
        op_output.tc_mutations.push(tc_mutation);
        Ok(op_output)
    } else {
        Err(ProcessOperationError::InvoiceIdExists)
    }
}

fn process_load_funds(token_channel: &mut TokenChannel,
                      send_funds_receipt: SendFundsReceipt) -> 
    Result<ProcessOperationOutput, ProcessOperationError> {

    // Check if the SendFundsReceipt is signed properly by us. 
    //      Could we somehow abstract this, for easier testing?
    if !send_funds_receipt.verify_signature(&token_channel.state().idents.local_public_key) {
        return Err(ProcessOperationError::InvalidSendFundsReceiptSignature);
    }

    let local_invoice_id = match token_channel.state().invoice.local_invoice_id {
        None => return Err(ProcessOperationError::MissingInvoiceId),
        Some(ref local_invoice_id) => local_invoice_id,
    };

    if local_invoice_id != &send_funds_receipt.invoice_id {
        return Err(ProcessOperationError::InvoiceIdMismatch);
    }

    // We are here if the invoice_id-s match!
    let mut op_output = ProcessOperationOutput {
        incoming_message: None,
        tc_mutations: Vec::new(),
    };

    // Clear local invoice id:
    let tc_mutation = TcMutation::ClearLocalInvoiceId;
    token_channel.mutate(&tc_mutation);
    op_output.tc_mutations.push(tc_mutation);

    // Add payment to self.balance.balance. We have to be careful because payment u128, and
    // self.balance.balance is of type i64.
    let payment = u64::try_from(send_funds_receipt.payment).unwrap_or(u64::max_value());
    let new_balance = token_channel.state().balance.balance.saturating_sub_unsigned(payment);

    let tc_mutation = TcMutation::SetBalance(new_balance);
    token_channel.mutate(&tc_mutation);
    op_output.tc_mutations.push(tc_mutation);

    Ok(op_output)
}

/// Make sure that we are open to incoming requests, 
/// and that the offered response proposal is high enough.
fn verify_local_send_price(token_channel: &TokenChannel,
                           route: &NeighborsRoute, 
                           pk_pair_position: &PkPairPosition) 
    -> Result<(), ProcessOperationError>  {

    let local_send_price = match token_channel.state().send_price.local_send_price {
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
fn process_request_send_message(token_channel: &mut TokenChannel,
                                request_send_msg: RequestSendMessage)
    -> Result<ProcessOperationOutput, ProcessOperationError> {

    // Make sure that the route does not contains cycles/duplicates:
    if !request_send_msg.route.is_cycle_free() {
        return Err(ProcessOperationError::DuplicateNodesInRoute);
    }

    // Find ourselves on the route. If we are not there, abort.
    let pk_pair = request_send_msg.route.find_pk_pair(
        &token_channel.state().idents.remote_public_key, 
        &token_channel.state().idents.local_public_key)
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

    verify_local_send_price(&*token_channel, &request_send_msg.route, &pk_pair)?;

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
    let new_remote_pending_debt = token_channel.state().balance.remote_pending_debt
        .checked_add(own_freeze_credits).ok_or(ProcessOperationError::CreditsCalcOverflow)?;

    if new_remote_pending_debt > token_channel.state().balance.remote_max_debt {
        return Err(ProcessOperationError::InsufficientTrust);
    }

    // Note that Verifying our own freezing link will be done outside. We don't have enough
    // information here to check this. In addition, even if it turns out we can't freeze those
    // credits, we don't want to create a token channel inconsistency.         
    
    let p_remote_requests = &token_channel.state().pending_requests.pending_remote_requests;
    // Make sure that we don't have this request as a pending request already:
    if p_remote_requests.contains_key(&request_send_msg.request_id) {
        return Err(ProcessOperationError::RequestAlreadyExists);
    }

    // Add pending request message:
    let pending_neighbor_request = request_send_msg.create_pending_request()
        .ok_or(ProcessOperationError::RequestContentTooLong)?;

    let mut op_output = ProcessOperationOutput {
        incoming_message: None,
        tc_mutations: Vec::new(),
    };

    let tc_mutation = TcMutation::InsertRemotePendingRequest(pending_neighbor_request);
    token_channel.mutate(&tc_mutation);
    op_output.tc_mutations.push(tc_mutation);

    // If we are here, we can freeze the credits:
    let tc_mutation = TcMutation::SetRemotePendingDebt(new_remote_pending_debt);
    token_channel.mutate(&tc_mutation);
    op_output.tc_mutations.push(tc_mutation);

    Ok(op_output)

}

fn process_response_send_message(token_channel: &mut TokenChannel,
                                 response_send_msg: ResponseSendMessage) ->
    Result<ProcessOperationOutput, ProcessOperationError> {

    // Make sure that id exists in local_pending hashmap, 
    // and access saved request details.
    let local_pending_requests = &token_channel.state().pending_requests.pending_local_requests;

    // Obtain pending request:
    let pending_request = local_pending_requests.get(&response_send_msg.request_id)
        .ok_or(ProcessOperationError::RequestDoesNotExist)?;

    let response_signature_buffer = create_response_signature_buffer(
                                        &response_send_msg,
                                        &pending_request);

    // Verify response message signature:
    if !verify_signature(&response_signature_buffer, 
                             &token_channel.state().idents.remote_public_key,
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
        &token_channel.state().idents.local_public_key, 
        &token_channel.state().idents.remote_public_key)
        .expect("Can not find myself in request's route!");

    let index = match pk_pair {
        PkPairPosition::Dest => pending_request.route.route_links.len().checked_add(1),
        PkPairPosition::NotDest(i) => i.checked_add(1),
    }.expect("Route too long!");

    let mut tc_mutations = Vec::new();

    let pending_request = token_channel
        .state()
        .pending_requests
        .pending_local_requests
        .get(&response_send_msg.request_id)
        .expect("pending_request not present!")
        .clone();

    // Remove entry from local_pending hashmap:
    let tc_mutation = TcMutation::RemoveLocalPendingRequest(response_send_msg.request_id.clone());
    token_channel.mutate(&tc_mutation);
    tc_mutations.push(tc_mutation);

    let next_index = index.checked_add(1).expect("Route too long!");
    let success_credits = credit_calc.credits_on_success(next_index, response_content_len)
        .expect("credits_on_success calculation failed!");
    let freeze_credits = credit_calc.credits_to_freeze(next_index)
        .expect("credits_to_freeze calculation failed!");

    // Decrease frozen credits and decrease balance:
    let new_local_pending_debt = 
        token_channel.state().balance.local_pending_debt.checked_sub(freeze_credits)
        .expect("Insufficient frozen credit!");

    let tc_mutation = TcMutation::SetLocalPendingDebt(new_local_pending_debt);
    token_channel.mutate(&tc_mutation);
    tc_mutations.push(tc_mutation);

    let new_balance = 
        token_channel.state().balance.balance.checked_sub_unsigned(success_credits)
        .expect("balance overflow");

    let tc_mutation = TcMutation::SetBalance(new_balance);
    token_channel.mutate(&tc_mutation);
    tc_mutations.push(tc_mutation);

    let incoming_message = Some(
        IncomingMessage::Response(
            IncomingResponseSendMessage {
                pending_request,
                incoming_response: response_send_msg,
            }
        )
    );

    Ok(ProcessOperationOutput {
        incoming_message,
        tc_mutations,
    })
}

fn process_failure_send_message(token_channel: &mut TokenChannel,
                                failure_send_msg: FailureSendMessage) ->
    Result<ProcessOperationOutput, ProcessOperationError> {
    
    // Make sure that id exists in local_pending hashmap, 
    // and access saved request details.
    let local_pending_requests = &token_channel.state().pending_requests.pending_local_requests;

    // Obtain pending request:
    let pending_request = local_pending_requests.get(&failure_send_msg.request_id)
        .ok_or(ProcessOperationError::RequestDoesNotExist)?;

    // Find ourselves on the route. If we are not there, abort.
    let pk_pair = pending_request.route.find_pk_pair(
        &token_channel.state().idents.local_public_key, 
        &token_channel.state().idents.remote_public_key)
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

    let mut tc_mutations = Vec::new();

    // Remove entry from local_pending hashmap:
    let pending_request = token_channel
        .state()
        .pending_requests
        .pending_local_requests
        .get(&failure_send_msg.request_id)
        .expect("pending_request not present!")
        .clone();

    // Remove entry from local_pending hashmap:
    let tc_mutation = TcMutation::RemoveLocalPendingRequest(failure_send_msg.request_id.clone());
    token_channel.mutate(&tc_mutation);
    tc_mutations.push(tc_mutation);


    let next_index = index.checked_add(1).expect("Route too long!");
    let failure_credits = credit_calc.credits_on_failure(next_index, reporting_index)
        .expect("credits_on_failure calculation failed!");
    let freeze_credits = credit_calc.credits_to_freeze(next_index)
        .expect("credits_to_freeze calculation failed!");

    // Decrease frozen credits and decrease balance:
    let new_local_pending_debt = 
        token_channel.state().balance.local_pending_debt.checked_sub(freeze_credits)
        .expect("Insufficient frozen credit!");

    let tc_mutation = TcMutation::SetLocalPendingDebt(new_local_pending_debt);
    token_channel.mutate(&tc_mutation);
    tc_mutations.push(tc_mutation);

    let new_balance = 
        token_channel.state().balance.balance.checked_sub_unsigned(failure_credits)
        .expect("balance overflow");

    let tc_mutation = TcMutation::SetBalance(new_balance);
    token_channel.mutate(&tc_mutation);
    tc_mutations.push(tc_mutation);
    
    // Return Failure message.
    let incoming_message = Some(
        IncomingMessage::Failure(
            IncomingFailureSendMessage {
                pending_request,
                incoming_failure: failure_send_msg,
            }
        )
    );

    Ok(ProcessOperationOutput {
        incoming_message,
        tc_mutations,
    })

}
