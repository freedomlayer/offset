#![allow(unused)]

use std::convert::TryFrom;
use std::collections::VecDeque;

use crypto::identity::verify_signature;
use crypto::hash;

use proto::funder::InvoiceId;
use proto::common::SendFundsReceipt;

use utils::safe_arithmetic::SafeArithmetic;
use utils::int_convert::usize_to_u32;

use super::types::{TokenChannel, TcMutation, 
    FriendMoveTokenInner, MAX_FUNDER_DEBT};
use super::super::credit_calc::CreditCalculator;
use super::super::types::{FriendTcOp, RequestSendFunds, 
    ResponseSendFunds, FailureSendFunds, PkPairPosition,
    PendingFriendRequest, FriendsRoute};
use super::super::signature_buff::{create_response_signature_buffer, 
    verify_failure_signature};
use super::super::messages::RequestsStatus;


/// Processes outgoing fundss for a token channel.
/// Used to batch as many fundss as possible.
pub struct OutgoingTokenChannel {
    token_channel: TokenChannel,
    // We want to limit the amount of bytes we can accumulate into
    // one MoveTokenChannel. Here we count how many bytes left until
    // we reach the maximum
    bytes_left: usize,  
    tc_mutations: Vec<TcMutation>,
    operations: Vec<FriendTcOp>,
}

pub enum QueueOperationError {
    RemoteMaxDebtTooLarge,
    InvoiceIdAlreadyExists,
    InvalidSendFundsReceiptSignature,
    MissingRemoteInvoiceId,
    InvoiceIdMismatch,
    DuplicateNodesInRoute,
    PkPairNotInRoute,
    InvalidFreezeLinks,
    RemoteIncomingRequestsDisabled,
    ResponsePaymentProposalTooLow,
    RouteTooLong,
    RouteEmpty,
    RequestContentTooLong,
    CreditCalculatorFailure,
    CreditsCalcOverflow,
    InsufficientTrust,
    RequestAlreadyExists,
    RequestDoesNotExist,
    InvalidResponseSignature,
    ProcessingFeeCollectedTooHigh,
    ResponseContentTooLong,
    ReportingNodeNonexistent,
    InvalidReportingNode,
    InvalidFailureSignature,
    FailureSentFromDest,
    MaxLengthReached,
    RemoteRequestsClosed,
}

pub struct QueueOperationFailure {
    pub operation: FriendTcOp,
    pub error: QueueOperationError,
}

/// A wrapper over a token channel, accumulating fundss to be sent as one transcation.
impl OutgoingTokenChannel {
    pub fn new(token_channel: &TokenChannel, move_token_max_length: usize) -> OutgoingTokenChannel {
        OutgoingTokenChannel {
            token_channel: token_channel.clone(),
            bytes_left: move_token_max_length,
            tc_mutations: Vec::new(),
            operations: Vec::new(),
        }
    }

    /// Finished queueing operations.
    /// Obtain the set of queued operations, and the expected side effects (mutations) on the token
    /// channel.
    pub fn done(self) -> (Vec<FriendTcOp>, Vec<TcMutation>) {
        (self.operations, self.tc_mutations)
    }

    pub fn queue_operation(&mut self, operation: FriendTcOp) ->
        Result<(), QueueOperationFailure> {

        // Check if we have room for another funds:
        let approx_bytes_count = operation.approx_bytes_count();
        if self.bytes_left < approx_bytes_count {
            return Err(QueueOperationFailure {
                operation,
                error: QueueOperationError::MaxLengthReached,
            });
        }

        let res = match operation.clone() {
            FriendTcOp::EnableRequests =>
                self.queue_enable_requests(),
            FriendTcOp::DisableRequests =>
                self.queue_disable_requests(),
            FriendTcOp::SetRemoteMaxDebt(proposed_max_debt) =>
                self.queue_set_remote_max_debt(proposed_max_debt),
            FriendTcOp::RequestSendFunds(request_send_funds) =>
                self.queue_request_send_funds(request_send_funds),
            FriendTcOp::ResponseSendFunds(response_send_funds) =>
                self.queue_response_send_funds(response_send_funds),
            FriendTcOp::FailureSendFunds(failure_send_funds) =>
                self.queue_failure_send_funds(failure_send_funds),
        };
        match res {
            Ok(tc_mutations) => {
                self.tc_mutations.extend(tc_mutations);
                self.operations.push(operation);
                self.bytes_left = self.bytes_left
                    .checked_sub(approx_bytes_count).unwrap();
                Ok(())
            },
            Err(error) => Err(QueueOperationFailure {
                operation,
                error,
            }),
        }
    }

    pub fn is_operations_empty(&self) -> bool {
        self.operations.is_empty()
    }

    fn queue_enable_requests(&mut self) ->
        Result<Vec<TcMutation>, QueueOperationError> {

        // TODO: Should we check first if local requests are already open?
        let mut tc_mutations = Vec::new();
        let tc_mutation = TcMutation::SetLocalRequestsStatus(RequestsStatus::Open);
        self.token_channel.mutate(&tc_mutation);
        tc_mutations.push(tc_mutation);

        Ok(tc_mutations)
    }

    fn queue_disable_requests(&mut self) ->
        Result<Vec<TcMutation>, QueueOperationError> {

        let mut tc_mutations = Vec::new();
        let tc_mutation = TcMutation::SetLocalRequestsStatus(RequestsStatus::Closed);
        self.token_channel.mutate(&tc_mutation);
        tc_mutations.push(tc_mutation);

        Ok(tc_mutations)
    }

    fn queue_set_remote_max_debt(&mut self, proposed_max_debt: u128) -> 
        Result<Vec<TcMutation>, QueueOperationError> {

        if proposed_max_debt > MAX_FUNDER_DEBT {
            return Err(QueueOperationError::RemoteMaxDebtTooLarge);
        }

        let mut tc_mutations = Vec::new();
        let tc_mutation = TcMutation::SetRemoteMaxDebt(proposed_max_debt);
        self.token_channel.mutate(&tc_mutation);
        tc_mutations.push(tc_mutation);
        Ok(tc_mutations)
    }


    fn queue_request_send_funds(&mut self, request_send_funds: RequestSendFunds) ->
        Result<Vec<TcMutation>, QueueOperationError> {

        // Make sure that the route does not contains cycles/duplicates:
        if !request_send_funds.route.is_cycle_free() {
            return Err(QueueOperationError::DuplicateNodesInRoute);
        }

        // Find ourselves on the route. If we are not there, abort.
        let local_index = request_send_funds.route.find_pk_pair(
            &self.token_channel.state().idents.local_public_key,
            &self.token_channel.state().idents.remote_public_key)
            .ok_or(QueueOperationError::PkPairNotInRoute)?;

        // Make sure that freeze_links and route_links are compatible in length:
        let freeze_links_len = request_send_funds.freeze_links.len();
        let route_links_len = request_send_funds.route.len();
        if freeze_links_len != local_index {
            return Err(QueueOperationError::InvalidFreezeLinks);
        }

        // Make sure that remote side is open to requests:
        if let RequestsStatus::Open = self.token_channel.state().requests_status.remote {
            return Err(QueueOperationError::RemoteRequestsClosed);
        }

        // Calculate amount of credits to freeze.
        let route_len = usize_to_u32(request_send_funds.route.len()) 
            .ok_or(QueueOperationError::RouteTooLong)?;
        let credit_calc = CreditCalculator::new(route_len,
                                                request_send_funds.dest_payment);

        // Get index of remote friend on the route:
        let remote_index = local_index.checked_add(1)
            .ok_or(QueueOperationError::RouteTooLong)?;
        let remote_index = usize_to_u32(remote_index)
            .ok_or(QueueOperationError::RouteTooLong)?;

        // Calculate amount of credits to freeze
        let own_freeze_credits = credit_calc.credits_to_freeze(remote_index)
            .ok_or(QueueOperationError::CreditCalculatorFailure)?;

        // Make sure we can freeze the credits
        let new_local_pending_debt = self.token_channel.state().balance.local_pending_debt
            .checked_add(own_freeze_credits).ok_or(QueueOperationError::CreditsCalcOverflow)?;

        if new_local_pending_debt > self.token_channel.state().balance.local_max_debt {
            return Err(QueueOperationError::InsufficientTrust);
        }

        let p_local_requests = &self.token_channel.state().pending_requests.pending_local_requests;
        // Make sure that we don't have this request as a pending request already:
        if p_local_requests.contains_key(&request_send_funds.request_id) {
            return Err(QueueOperationError::RequestAlreadyExists);
        }

        // Add pending request funds:
        let pending_friend_request = request_send_funds.create_pending_request();

        let mut tc_mutations = Vec::new();
        let tc_mutation = TcMutation::InsertLocalPendingRequest(pending_friend_request);
        self.token_channel.mutate(&tc_mutation);
        tc_mutations.push(tc_mutation);
        
        // If we are here, we can freeze the credits:
        let tc_mutation = TcMutation::SetLocalPendingDebt(new_local_pending_debt);
        self.token_channel.mutate(&tc_mutation);
        tc_mutations.push(tc_mutation);

        Ok(tc_mutations)
    }

    fn queue_response_send_funds(&mut self, response_send_funds: ResponseSendFunds) ->
        Result<Vec<TcMutation>, QueueOperationError> {
        // Make sure that id exists in remote_pending hashmap, 
        // and access saved request details.
        let remote_pending_requests = &self.token_channel.state()
            .pending_requests.pending_remote_requests;

        // Obtain pending request:
        let pending_request = remote_pending_requests.get(&response_send_funds.request_id)
            .ok_or(QueueOperationError::RequestDoesNotExist)?
            .clone();
        // TODO: Possibly get rid of clone() here for optimization later

        // verify signature:
        let response_signature_buffer = create_response_signature_buffer(
                                            &response_send_funds,
                                            &pending_request);

        // Verify response funds signature:
        if !verify_signature(&response_signature_buffer, 
                                 &self.token_channel.state().idents.local_public_key,
                                 &response_send_funds.signature) {
            return Err(QueueOperationError::InvalidResponseSignature);
        }

        // Calculate amount of credits to freeze.
        let route_len = usize_to_u32(pending_request.route.len()) 
            .ok_or(QueueOperationError::RouteTooLong)?;
        let credit_calc = CreditCalculator::new(route_len,
                                                pending_request.dest_payment);

        // Find ourselves on the route. If we are not there, abort.
        let remote_index = pending_request.route.find_pk_pair(
            &self.token_channel.state().idents.remote_public_key, 
            &self.token_channel.state().idents.local_public_key).unwrap();

        let local_index = usize_to_u32(remote_index.checked_add(1).unwrap()).unwrap();

        // Remove entry from remote_pending hashmap:
        let mut tc_mutations = Vec::new();
        let tc_mutation = TcMutation::RemoveRemotePendingRequest(response_send_funds.request_id);
        self.token_channel.mutate(&tc_mutation);
        tc_mutations.push(tc_mutation);

        let success_credits = credit_calc.credits_on_success(local_index).unwrap();
        let freeze_credits = credit_calc.credits_to_freeze(local_index).unwrap();


        // Decrease frozen credits and increase balance:
        let new_remote_pending_debt = 
            self.token_channel.state().balance.remote_pending_debt.checked_sub(freeze_credits)
            .expect("Insufficient frozen credit!");

        let tc_mutation = TcMutation::SetRemotePendingDebt(new_remote_pending_debt);
        self.token_channel.mutate(&tc_mutation);
        tc_mutations.push(tc_mutation);

        let new_balance = 
            self.token_channel.state().balance.balance.checked_add_unsigned(success_credits)
            .expect("balance overflow");

        let tc_mutation = TcMutation::SetBalance(new_balance);
        self.token_channel.mutate(&tc_mutation);
        tc_mutations.push(tc_mutation);

        Ok(tc_mutations)
    }

    fn queue_failure_send_funds(&mut self, failure_send_funds: FailureSendFunds) ->
        Result<Vec<TcMutation>, QueueOperationError> {
        // Make sure that id exists in remote_pending hashmap, 
        // and access saved request details.
        let remote_pending_requests = &self.token_channel.state().pending_requests
            .pending_remote_requests;

        // Obtain pending request:
        let pending_request = remote_pending_requests.get(&failure_send_funds.request_id)
            .ok_or(QueueOperationError::RequestDoesNotExist)?;

        // Find ourselves on the route. If we are not there, abort.
        let remote_index = pending_request.route.find_pk_pair(
            &self.token_channel.state().idents.remote_public_key,
            &self.token_channel.state().idents.local_public_key).unwrap();

        let local_index = remote_index.checked_add(1).unwrap();
        if local_index.checked_add(1).unwrap() == pending_request.route.len() {
            // Note that we can not be the destination. The destination can not be the sender of a
            // failure funds.
            return Err(QueueOperationError::FailureSentFromDest);
        }

        // Make sure that reporting node public key is:
        //  - inside the route
        //  - After us on the route, or us.
        //  - Not the destination node
        
        let reporting_index = pending_request.route.pk_to_index(
            &failure_send_funds.reporting_public_key)
            .ok_or(QueueOperationError::ReportingNodeNonexistent)?;


        if reporting_index < local_index {
            return Err(QueueOperationError::InvalidReportingNode);
        }

        verify_failure_signature(&failure_send_funds, &pending_request)
            .ok_or(QueueOperationError::InvalidFailureSignature)?;

        // At this point we believe the failure funds is valid.
        let route_len = usize_to_u32(pending_request.route.len()) 
            .ok_or(QueueOperationError::RouteTooLong)?;
        let credit_calc = CreditCalculator::new(route_len,
                                                pending_request.dest_payment);


        // Remove entry from remote hashmap:
        let mut tc_mutations = Vec::new();

        let tc_mutation = TcMutation::RemoveRemotePendingRequest(failure_send_funds.request_id);
        self.token_channel.mutate(&tc_mutation);
        tc_mutations.push(tc_mutation);

        let local_index = usize_to_u32(local_index).unwrap();
        let reporting_index = usize_to_u32(reporting_index).unwrap();

        let failure_credits = credit_calc.credits_on_failure(local_index, reporting_index).unwrap();
        let freeze_credits = credit_calc.credits_to_freeze(local_index).unwrap();

        // Decrease frozen credits:
        let new_remote_pending_debt = 
            self.token_channel.state().balance.remote_pending_debt.checked_sub(freeze_credits)
            .unwrap();

        let tc_mutation = TcMutation::SetRemotePendingDebt(new_remote_pending_debt);
        self.token_channel.mutate(&tc_mutation);
        tc_mutations.push(tc_mutation);

        // Add to balance:
        let new_balance = 
            self.token_channel.state().balance.balance.checked_add_unsigned(failure_credits)
            .unwrap();

        let tc_mutation = TcMutation::SetBalance(new_balance);
        self.token_channel.mutate(&tc_mutation);
        tc_mutations.push(tc_mutation);

        Ok(tc_mutations)
    }
}

