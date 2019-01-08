use crypto::identity::verify_signature;

use common::int_convert::usize_to_u32;
use common::safe_arithmetic::SafeSignedArithmetic;

use proto::verify::Verify;
use proto::funder::messages::{RequestSendFunds, ResponseSendFunds, FailureSendFunds,
    FriendTcOp, PendingRequest, RequestsStatus};
use proto::funder::signature_buff::{create_response_signature_buffer};

use crate::types::create_pending_request;

use crate::credit_calc::CreditCalculator;

use super::types::{MutualCredit, MAX_FUNDER_DEBT, McMutation};


/*
pub struct IncomingRequestSendFunds {
    pub request: PendingRequest,
}
*/

#[derive(Debug)]
pub struct IncomingResponseSendFunds {
    pub pending_request: PendingRequest,
    pub incoming_response: ResponseSendFunds,
}

#[derive(Debug)]
pub struct IncomingFailureSendFunds {
    pub pending_request: PendingRequest,
    pub incoming_failure: FailureSendFunds,
}

#[derive(Debug)]
pub enum IncomingMessage {
    Request(RequestSendFunds),
    Response(IncomingResponseSendFunds),
    Failure(IncomingFailureSendFunds),
}

/// Resulting tasks to perform after processing an incoming operation.
#[allow(unused)]
pub struct ProcessOperationOutput {
    pub incoming_message: Option<IncomingMessage>,
    pub mc_mutations: Vec<McMutation>,
}


#[derive(Debug)]
pub enum ProcessOperationError {
    RemoteMaxDebtTooLarge(u128),
    /// Trying to set the invoiceId, while already expecting another invoice id.
    PkPairNotInRoute,
    /// The Route contains some public key twice.
    InvalidRoute,
    RequestsAlreadyDisabled,
    RouteTooLong,
    InsufficientTrust,
    CreditsCalcOverflow,
    InvalidFreezeLinks,
    CreditCalculatorFailure,
    RequestAlreadyExists,
    RequestDoesNotExist,
    InvalidResponseSignature,
    ReportingNodeNonexistent,
    InvalidReportingNode,
    InvalidFailureSignature,
    LocalRequestsClosed,
}

#[derive(Debug)]
pub struct ProcessTransListError {
    index: usize,
    process_trans_error: ProcessOperationError,
}


pub fn process_operations_list(mutual_credit: &mut MutualCredit, 
                                        operations: Vec<FriendTcOp>) ->
    Result<Vec<ProcessOperationOutput>, ProcessTransListError> {

    let mut outputs = Vec::new();

    // We do not change the original MutualCredit. 
    // Instead, we are operating over a clone:
    // This operation is not very expensive, because we are using immutable data structures
    // (specifically, HashMaps).

    for (index, funds) in operations.into_iter().enumerate() {
        match process_operation(mutual_credit, funds) {
            Err(e) => return Err(ProcessTransListError {
                index,
                process_trans_error: e
            }),
            Ok(trans_output) => outputs.push(trans_output),
        }
    }
    Ok(outputs)
}

pub fn process_operation(mutual_credit: &mut MutualCredit, friend_tc_op: FriendTcOp) ->
    Result<ProcessOperationOutput, ProcessOperationError> {
    match friend_tc_op {
        FriendTcOp::EnableRequests =>
            process_enable_requests(mutual_credit),
        FriendTcOp::DisableRequests =>
            process_disable_requests(mutual_credit),
        FriendTcOp::SetRemoteMaxDebt(proposed_max_debt) =>
            process_set_remote_max_debt(mutual_credit, proposed_max_debt),
        FriendTcOp::RequestSendFunds(request_send_funds) =>
            process_request_send_funds(mutual_credit, request_send_funds),
        FriendTcOp::ResponseSendFunds(response_send_funds) =>
            process_response_send_funds(mutual_credit, response_send_funds),
        FriendTcOp::FailureSendFunds(failure_send_funds) =>
            process_failure_send_funds(mutual_credit, failure_send_funds),
    }
}

fn process_enable_requests(mutual_credit: &mut MutualCredit) ->
    Result<ProcessOperationOutput, ProcessOperationError> {

    let mut op_output = ProcessOperationOutput {
        incoming_message: None,
        mc_mutations: Vec::new(),
    };
    let tc_mutation = McMutation::SetRemoteRequestsStatus(RequestsStatus::Open);
    mutual_credit.mutate(&tc_mutation);
    op_output.mc_mutations.push(tc_mutation);

    Ok(op_output)
}

fn process_disable_requests(mutual_credit: &mut MutualCredit) ->
    Result<ProcessOperationOutput, ProcessOperationError> {

    let mut op_output = ProcessOperationOutput {
        incoming_message: None,
        mc_mutations: Vec::new(),
    };

    match mutual_credit.state().requests_status.remote {
        RequestsStatus::Open => {
            let tc_mutation = McMutation::SetRemoteRequestsStatus(RequestsStatus::Closed);
            mutual_credit.mutate(&tc_mutation);
            op_output.mc_mutations.push(tc_mutation);
            Ok(op_output)
        },
        RequestsStatus::Closed => Err(ProcessOperationError::RequestsAlreadyDisabled),
    }
}

fn process_set_remote_max_debt(mutual_credit: &mut MutualCredit,
                               proposed_max_debt: u128) -> 
    Result<ProcessOperationOutput, ProcessOperationError> {

    let mut op_output = ProcessOperationOutput {
        incoming_message: None,
        mc_mutations: Vec::new(),
    };

    if proposed_max_debt > MAX_FUNDER_DEBT {
        Err(ProcessOperationError::RemoteMaxDebtTooLarge(proposed_max_debt))
    } else {
        let tc_mutation = McMutation::SetLocalMaxDebt(proposed_max_debt);
        mutual_credit.mutate(&tc_mutation);
        op_output.mc_mutations.push(tc_mutation);
        Ok(op_output)
    }
}


/// Process an incoming RequestSendFunds
fn process_request_send_funds(mutual_credit: &mut MutualCredit,
                                request_send_funds: RequestSendFunds)
    -> Result<ProcessOperationOutput, ProcessOperationError> {

    if !request_send_funds.route.is_valid() {
        return Err(ProcessOperationError::InvalidRoute);
    }

    // Find ourselves on the route. If we are not there, abort.
    let remote_index = request_send_funds.route.find_pk_pair(
        &mutual_credit.state().idents.remote_public_key, 
        &mutual_credit.state().idents.local_public_key)
        .ok_or(ProcessOperationError::PkPairNotInRoute)?;

    // Make sure that freeze_links and route_links are compatible in length:
    let freeze_links_len = request_send_funds.freeze_links.len();
    if remote_index.checked_add(1).unwrap() != freeze_links_len {
        return Err(ProcessOperationError::InvalidFreezeLinks);
    }

    // Make sure that we are open to requests:
    if !mutual_credit.state().requests_status.local.is_open() {
        return Err(ProcessOperationError::LocalRequestsClosed);
    }

    let route_len = usize_to_u32(request_send_funds.route.len())
        .ok_or(ProcessOperationError::RouteTooLong)?;
    let credit_calc = CreditCalculator::new(route_len,
                                            request_send_funds.dest_payment);

    let local_index = remote_index.checked_add(1)
        .ok_or(ProcessOperationError::RouteTooLong)?;
    let local_index = usize_to_u32(local_index)
        .ok_or(ProcessOperationError::RouteTooLong)?;

    // Calculate amount of credits to freeze
    let own_freeze_credits = credit_calc.credits_to_freeze(local_index)
        .ok_or(ProcessOperationError::CreditCalculatorFailure)?;

    // Make sure we can freeze the credits
    let new_remote_pending_debt = mutual_credit.state().balance.remote_pending_debt
        .checked_add(own_freeze_credits).ok_or(ProcessOperationError::CreditsCalcOverflow)?;

    if new_remote_pending_debt > mutual_credit.state().balance.remote_max_debt {
        return Err(ProcessOperationError::InsufficientTrust);
    }

    // Note that Verifying our own freezing link will be done outside. We don't have enough
    // information here to check this. In addition, even if it turns out we can't freeze those
    // credits, we don't want to create a token channel inconsistency.         
    
    let p_remote_requests = &mutual_credit.state().pending_requests.pending_remote_requests;
    // Make sure that we don't have this request as a pending request already:
    if p_remote_requests.contains_key(&request_send_funds.request_id) {
        return Err(ProcessOperationError::RequestAlreadyExists);
    }

    // Add pending request funds:
    let pending_friend_request = create_pending_request(&request_send_funds);

    let mut op_output = ProcessOperationOutput {
        incoming_message: Some(IncomingMessage::Request(request_send_funds)),
        mc_mutations: Vec::new(),
    };

    let tc_mutation = McMutation::InsertRemotePendingRequest(pending_friend_request);
    mutual_credit.mutate(&tc_mutation);
    op_output.mc_mutations.push(tc_mutation);

    // If we are here, we can freeze the credits:
    let tc_mutation = McMutation::SetRemotePendingDebt(new_remote_pending_debt);
    mutual_credit.mutate(&tc_mutation);
    op_output.mc_mutations.push(tc_mutation);

    Ok(op_output)

}

fn process_response_send_funds(mutual_credit: &mut MutualCredit,
                                 response_send_funds: ResponseSendFunds) ->
    Result<ProcessOperationOutput, ProcessOperationError> {

    // Make sure that id exists in local_pending hashmap, 
    // and access saved request details.
    let local_pending_requests = &mutual_credit.state().pending_requests.pending_local_requests;

    // Obtain pending request:
    // TODO: Possibly get rid of clone() here for optimization later
    let pending_request = local_pending_requests
        .get(&response_send_funds.request_id)
        .ok_or(ProcessOperationError::RequestDoesNotExist)?
        .clone();

    let dest_public_key = pending_request.route.public_keys
        .last()
        .unwrap();

    let response_signature_buffer = create_response_signature_buffer(
                                        &response_send_funds,
                                        &pending_request);

    // Verify response funds signature:
    if !verify_signature(&response_signature_buffer, 
                             dest_public_key,
                             &response_send_funds.signature) {
        return Err(ProcessOperationError::InvalidResponseSignature);
    }

    // It should never happen that usize_to_u32 fails here, because we 
    // checked this when we created the pending_request.
    let route_len = usize_to_u32(pending_request.route.len()).unwrap();
    let credit_calc = CreditCalculator::new(route_len,
                                            pending_request.dest_payment);

    // Find ourselves on the route. If we are not there, abort.
    let local_index = pending_request.route.find_pk_pair(
        &mutual_credit.state().idents.local_public_key, 
        &mutual_credit.state().idents.remote_public_key).unwrap();

    let mut mc_mutations = Vec::new();

    // Remove entry from local_pending hashmap:
    let tc_mutation = McMutation::RemoveLocalPendingRequest(response_send_funds.request_id);
    mutual_credit.mutate(&tc_mutation);
    mc_mutations.push(tc_mutation);

    let remote_index = usize_to_u32(local_index.checked_add(1).unwrap()).unwrap();
    let success_credits = credit_calc.credits_on_success(remote_index).unwrap();
    let freeze_credits = credit_calc.credits_to_freeze(remote_index).unwrap();

    // Decrease frozen credits and decrease balance:
    let new_local_pending_debt = 
        mutual_credit.state().balance.local_pending_debt
        .checked_sub(freeze_credits)
        .unwrap();

    let tc_mutation = McMutation::SetLocalPendingDebt(new_local_pending_debt);
    mutual_credit.mutate(&tc_mutation);
    mc_mutations.push(tc_mutation);

    let new_balance = 
        mutual_credit.state().balance.balance
        .checked_sub_unsigned(success_credits)
        .unwrap();

    let tc_mutation = McMutation::SetBalance(new_balance);
    mutual_credit.mutate(&tc_mutation);
    mc_mutations.push(tc_mutation);

    let incoming_message = Some(
        IncomingMessage::Response(
            IncomingResponseSendFunds {
                pending_request,
                incoming_response: response_send_funds,
            }
        )
    );

    Ok(ProcessOperationOutput {
        incoming_message,
        mc_mutations,
    })
}

fn process_failure_send_funds(mutual_credit: &mut MutualCredit,
                                failure_send_funds: FailureSendFunds) ->
    Result<ProcessOperationOutput, ProcessOperationError> {
    
    // Make sure that id exists in local_pending hashmap, 
    // and access saved request details.
    let local_pending_requests = &mutual_credit.state().pending_requests.pending_local_requests;

    // Obtain pending request:
    let pending_request = local_pending_requests
        .get(&failure_send_funds.request_id)
        .ok_or(ProcessOperationError::RequestDoesNotExist)?
        .clone();
    // TODO: Possibly get rid of clone() here for optimization later

    // Find ourselves on the route. If we are not there, abort.
    let local_index = pending_request.route.find_pk_pair(
        &mutual_credit.state().idents.local_public_key, 
        &mutual_credit.state().idents.remote_public_key).unwrap();

    // Make sure that reporting node public key is:
    //  - inside the route
    //  - After us on the route.
    //  - Not the destination node
    
    let reporting_index = pending_request.route.pk_to_index(
        &failure_send_funds.reporting_public_key)
        .ok_or(ProcessOperationError::ReportingNodeNonexistent)?;

    if reporting_index <= local_index {
        return Err(ProcessOperationError::InvalidReportingNode);
    }


    let pair = (&failure_send_funds, &pending_request);
    if !pair.verify() {
        return Err(ProcessOperationError::InvalidFailureSignature);
    }

    // At this point we believe the failure funds is valid.
    let route_len = usize_to_u32(pending_request.route.len()).unwrap();
    let credit_calc = CreditCalculator::new(route_len,
                                            pending_request.dest_payment);

    let mut mc_mutations = Vec::new();

    // Remove entry from local_pending hashmap:
    let tc_mutation = McMutation::RemoveLocalPendingRequest(failure_send_funds.request_id);
    mutual_credit.mutate(&tc_mutation);
    mc_mutations.push(tc_mutation);


    let remote_index = usize_to_u32(local_index.checked_add(1).unwrap()).unwrap();
    let reporting_index = usize_to_u32(reporting_index).unwrap();
    let failure_credits = credit_calc.credits_on_failure(remote_index, reporting_index)
        .unwrap();
    let freeze_credits = credit_calc.credits_to_freeze(remote_index)
        .unwrap();

    // Decrease frozen credits and decrease balance:
    let new_local_pending_debt = 
        mutual_credit.state().balance.local_pending_debt.checked_sub(freeze_credits)
        .unwrap();

    let tc_mutation = McMutation::SetLocalPendingDebt(new_local_pending_debt);
    mutual_credit.mutate(&tc_mutation);
    mc_mutations.push(tc_mutation);

    let new_balance = 
        mutual_credit.state().balance.balance.checked_sub_unsigned(failure_credits)
        .unwrap();

    let tc_mutation = McMutation::SetBalance(new_balance);
    mutual_credit.mutate(&tc_mutation);
    mc_mutations.push(tc_mutation);
    
    // Return Failure funds.
    let incoming_message = Some(
        IncomingMessage::Failure(
            IncomingFailureSendFunds {
                pending_request,
                incoming_failure: failure_send_funds,
            }
        )
    );

    Ok(ProcessOperationOutput {
        incoming_message,
        mc_mutations,
    })

}
