use crypto::identity::verify_signature;

use common::int_convert::usize_to_u32;
use common::safe_arithmetic::SafeSignedArithmetic;

use proto::funder::messages::{
    CancelSendFundsOp, CommitSendFundsOp, FriendTcOp, RequestSendFundsOp, RequestsStatus,
    ResponseSendFundsOp,
};
use proto::funder::signature_buff::create_response_signature_buffer;

use super::types::{McMutation, MutualCredit, MAX_FUNDER_DEBT};
use crate::credit_calc::CreditCalculator;

/// Processes outgoing funds for a token channel.
/// Used to batch as many funds as possible.
pub struct OutgoingMc {
    mutual_credit: MutualCredit,
}

#[derive(Debug)]
pub enum QueueOperationError {
    RemoteMaxDebtTooLarge,
    InvalidRoute,
    PkPairNotInRoute,
    RouteTooLong,
    CreditCalculatorFailure,
    CreditsCalcOverflow,
    InsufficientTrust,
    RequestAlreadyExists,
    RequestDoesNotExist,
    InvalidResponseSignature,
    ReportingNodeNonexistent,
    InvalidReportingNode,
    InvalidFailureSignature,
    FailureSentFromDest,
    RemoteRequestsClosed,
}

/// A wrapper over a token channel, accumulating funds to be sent as one transaction.
impl OutgoingMc {
    pub fn new(mutual_credit: &MutualCredit) -> OutgoingMc {
        OutgoingMc {
            mutual_credit: mutual_credit.clone(),
        }
    }

    pub fn queue_operation(
        &mut self,
        operation: &FriendTcOp,
    ) -> Result<Vec<McMutation>, QueueOperationError> {
        // TODO: Maybe remove clone from here later:
        match operation.clone() {
            FriendTcOp::EnableRequests => self.queue_enable_requests(),
            FriendTcOp::DisableRequests => self.queue_disable_requests(),
            FriendTcOp::SetRemoteMaxDebt(proposed_max_debt) => {
                self.queue_set_remote_max_debt(proposed_max_debt)
            }
            FriendTcOp::RequestSendFunds(request_send_funds) => {
                self.queue_request_send_funds(request_send_funds)
            }
            FriendTcOp::ResponseSendFunds(response_send_funds) => {
                self.queue_response_send_funds(response_send_funds)
            }
            FriendTcOp::CancelSendFunds(cancel_send_funds) => {
                self.queue_cancel_send_funds(cancel_send_funds)
            }
            FriendTcOp::CommitSendFunds(commit_send_funds) => {
                self.queue_commit_send_funds(commit_send_funds)
            }
        }
    }

    fn queue_enable_requests(&mut self) -> Result<Vec<McMutation>, QueueOperationError> {
        // TODO: Should we check first if local requests are already open?
        let mut tc_mutations = Vec::new();
        let tc_mutation = McMutation::SetLocalRequestsStatus(RequestsStatus::Open);
        self.mutual_credit.mutate(&tc_mutation);
        tc_mutations.push(tc_mutation);

        Ok(tc_mutations)
    }

    fn queue_disable_requests(&mut self) -> Result<Vec<McMutation>, QueueOperationError> {
        let mut tc_mutations = Vec::new();
        let tc_mutation = McMutation::SetLocalRequestsStatus(RequestsStatus::Closed);
        self.mutual_credit.mutate(&tc_mutation);
        tc_mutations.push(tc_mutation);

        Ok(tc_mutations)
    }

    fn queue_set_remote_max_debt(
        &mut self,
        proposed_max_debt: u128,
    ) -> Result<Vec<McMutation>, QueueOperationError> {
        if proposed_max_debt > MAX_FUNDER_DEBT {
            return Err(QueueOperationError::RemoteMaxDebtTooLarge);
        }

        let mut tc_mutations = Vec::new();
        let tc_mutation = McMutation::SetRemoteMaxDebt(proposed_max_debt);
        self.mutual_credit.mutate(&tc_mutation);
        tc_mutations.push(tc_mutation);
        Ok(tc_mutations)
    }

    fn queue_request_send_funds(
        &mut self,
        request_send_funds: RequestSendFundsOp,
    ) -> Result<Vec<McMutation>, QueueOperationError> {
        if !request_send_funds.route.is_valid() {
            return Err(QueueOperationError::InvalidRoute);
        }

        // Find ourselves on the route. If we are not there, abort.
        let local_index = request_send_funds
            .route
            .find_pk_pair(
                &self.mutual_credit.state().idents.local_public_key,
                &self.mutual_credit.state().idents.remote_public_key,
            )
            .ok_or(QueueOperationError::PkPairNotInRoute)?;

        // Make sure that remote side is open to requests:
        if !self.mutual_credit.state().requests_status.remote.is_open() {
            return Err(QueueOperationError::RemoteRequestsClosed);
        }

        // Calculate amount of credits to freeze.
        let route_len = usize_to_u32(request_send_funds.route.len())
            .ok_or(QueueOperationError::RouteTooLong)?;
        let credit_calc = CreditCalculator::new(route_len, request_send_funds.dest_payment);

        // Get index of remote friend on the route:
        let remote_index = local_index
            .checked_add(1)
            .ok_or(QueueOperationError::RouteTooLong)?;
        let remote_index = usize_to_u32(remote_index).ok_or(QueueOperationError::RouteTooLong)?;

        // Calculate amount of credits to freeze
        let own_freeze_credits = credit_calc
            .credits_to_freeze(remote_index)
            .ok_or(QueueOperationError::CreditCalculatorFailure)?;

        let balance = &self.mutual_credit.state().balance;

        // Make sure we can freeze the credits
        let new_local_pending_debt = balance
            .local_pending_debt
            .checked_add(own_freeze_credits)
            .ok_or(QueueOperationError::CreditsCalcOverflow)?;

        // Check that local_pending_debt - balance <= local_max_debt:
        let sub = balance
            .balance
            .checked_sub_unsigned(new_local_pending_debt)
            .ok_or(QueueOperationError::CreditsCalcOverflow)?;

        if sub
            .checked_add_unsigned(balance.local_max_debt)
            .ok_or(QueueOperationError::CreditsCalcOverflow)?
            < 0
        {
            return Err(QueueOperationError::InsufficientTrust);
        }

        let p_local_requests = &self
            .mutual_credit
            .state()
            .pending_transactions
            .pending_local_requests;
        // Make sure that we don't have this request as a pending request already:
        if p_local_requests.contains_key(&request_send_funds.request_id) {
            return Err(QueueOperationError::RequestAlreadyExists);
        }

        unimplemented!();

        /*

        // Add pending request funds:
        let pending_friend_request = create_pending_transaction(&request_send_funds);

        let mut tc_mutations = Vec::new();
        let tc_mutation = McMutation::InsertLocalPendingTransaction(pending_friend_request);
        self.mutual_credit.mutate(&tc_mutation);
        tc_mutations.push(tc_mutation);

        // If we are here, we can freeze the credits:
        let tc_mutation = McMutation::SetLocalPendingDebt(new_local_pending_debt);
        self.mutual_credit.mutate(&tc_mutation);
        tc_mutations.push(tc_mutation);

        Ok(tc_mutations)
        */
    }

    fn queue_response_send_funds(
        &mut self,
        response_send_funds: ResponseSendFundsOp,
    ) -> Result<Vec<McMutation>, QueueOperationError> {
        // Make sure that id exists in remote_pending hashmap,
        // and access saved request details.
        let remote_pending_transactions = &self
            .mutual_credit
            .state()
            .pending_transactions
            .pending_remote_requests;

        // Obtain pending request:
        let pending_transaction = remote_pending_transactions
            .get(&response_send_funds.request_id)
            .ok_or(QueueOperationError::RequestDoesNotExist)?
            .clone();
        // TODO: Possibly get rid of clone() here for optimization later

        // verify signature:
        let response_signature_buffer =
            create_response_signature_buffer(&response_send_funds, &pending_transaction);
        // The response was signed by the destination node:
        let dest_public_key = pending_transaction.route.public_keys.last().unwrap();

        // Verify response funds signature:
        if !verify_signature(
            &response_signature_buffer,
            dest_public_key,
            &response_send_funds.signature,
        ) {
            return Err(QueueOperationError::InvalidResponseSignature);
        }

        // Calculate amount of credits to freeze.
        let route_len = usize_to_u32(pending_transaction.route.len())
            .ok_or(QueueOperationError::RouteTooLong)?;
        let credit_calc = CreditCalculator::new(route_len, pending_transaction.dest_payment);

        // Find ourselves on the route. If we are not there, abort.
        let remote_index = pending_transaction
            .route
            .find_pk_pair(
                &self.mutual_credit.state().idents.remote_public_key,
                &self.mutual_credit.state().idents.local_public_key,
            )
            .unwrap();

        let local_index = usize_to_u32(remote_index.checked_add(1).unwrap()).unwrap();

        // Remove entry from remote_pending hashmap:
        let mut tc_mutations = Vec::new();
        let tc_mutation =
            McMutation::RemoveRemotePendingTransaction(response_send_funds.request_id);
        self.mutual_credit.mutate(&tc_mutation);
        tc_mutations.push(tc_mutation);

        let success_credits = credit_calc.credits_on_success(local_index).unwrap();
        let freeze_credits = credit_calc.credits_to_freeze(local_index).unwrap();

        // Decrease frozen credits and increase balance:
        let new_remote_pending_debt = self
            .mutual_credit
            .state()
            .balance
            .remote_pending_debt
            .checked_sub(freeze_credits)
            .expect("Insufficient frozen credit!");

        let tc_mutation = McMutation::SetRemotePendingDebt(new_remote_pending_debt);
        self.mutual_credit.mutate(&tc_mutation);
        tc_mutations.push(tc_mutation);

        let new_balance = self
            .mutual_credit
            .state()
            .balance
            .balance
            .checked_add_unsigned(success_credits)
            .expect("balance overflow");

        let tc_mutation = McMutation::SetBalance(new_balance);
        self.mutual_credit.mutate(&tc_mutation);
        tc_mutations.push(tc_mutation);

        Ok(tc_mutations)
    }

    fn queue_cancel_send_funds(
        &mut self,
        cancel_send_funds: CancelSendFundsOp,
    ) -> Result<Vec<McMutation>, QueueOperationError> {
        // Make sure that id exists in remote_pending hashmap,
        // and access saved request details.
        let remote_pending_transactions = &self
            .mutual_credit
            .state()
            .pending_transactions
            .pending_remote_requests;

        // Obtain pending request:
        let pending_transaction = remote_pending_transactions
            .get(&cancel_send_funds.request_id)
            .ok_or(QueueOperationError::RequestDoesNotExist)?;

        // Find ourselves on the route. If we are not there, abort.
        let remote_index = pending_transaction
            .route
            .find_pk_pair(
                &self.mutual_credit.state().idents.remote_public_key,
                &self.mutual_credit.state().idents.local_public_key,
            )
            .unwrap();

        let local_index = remote_index.checked_add(1).unwrap();
        if local_index.checked_add(1).unwrap() == pending_transaction.route.len() {
            // Note that we can not be the destination. The destination can not be the sender of a
            // failure funds.
            return Err(QueueOperationError::FailureSentFromDest);
        }

        // Make sure that reporting node public key is:
        //  - inside the route
        //  - After us on the route, or us.
        //  - Not the destination node
        unimplemented!();

        /*

        let reporting_index = pending_transaction
            .route
            .pk_to_index(&cancel_send_funds.reporting_public_key)
            .ok_or(QueueOperationError::ReportingNodeNonexistent)?;

        if reporting_index < local_index {
            return Err(QueueOperationError::InvalidReportingNode);
        }


        verify_failure_signature(&cancel_send_funds, &pending_transaction)
            .ok_or(QueueOperationError::InvalidFailureSignature)?;

        // At this point we believe the failure funds is valid.
        let route_len = usize_to_u32(pending_transaction.route.len())
            .ok_or(QueueOperationError::RouteTooLong)?;
        let credit_calc = CreditCalculator::new(route_len, pending_transaction.dest_payment);

        // Remove entry from remote hashmap:
        let mut tc_mutations = Vec::new();

        let tc_mutation = McMutation::RemoveRemotePendingTransaction(cancel_send_funds.request_id);
        self.mutual_credit.mutate(&tc_mutation);
        tc_mutations.push(tc_mutation);

        let local_index = usize_to_u32(local_index).unwrap();
        let reporting_index = usize_to_u32(reporting_index).unwrap();

        let failure_credits = credit_calc
            .credits_on_failure(local_index, reporting_index)
            .unwrap();
        let freeze_credits = credit_calc.credits_to_freeze(local_index).unwrap();

        // Decrease frozen credits:
        let new_remote_pending_debt = self
            .mutual_credit
            .state()
            .balance
            .remote_pending_debt
            .checked_sub(freeze_credits)
            .unwrap();

        let tc_mutation = McMutation::SetRemotePendingDebt(new_remote_pending_debt);
        self.mutual_credit.mutate(&tc_mutation);
        tc_mutations.push(tc_mutation);

        // Add to balance:
        let new_balance = self
            .mutual_credit
            .state()
            .balance
            .balance
            .checked_add_unsigned(failure_credits)
            .unwrap();

        let tc_mutation = McMutation::SetBalance(new_balance);
        self.mutual_credit.mutate(&tc_mutation);
        tc_mutations.push(tc_mutation);

        Ok(tc_mutations)
        */
    }

    fn queue_commit_send_funds(
        &mut self,
        commit_send_funds: CommitSendFundsOp,
    ) -> Result<Vec<McMutation>, QueueOperationError> {
        unimplemented!();
    }
}
