use crypto::hash_lock::HashLock;
use crypto::identity::verify_signature;

use common::safe_arithmetic::SafeSignedArithmetic;

use proto::funder::messages::{
    CancelSendFundsOp, FriendTcOp, RequestSendFundsOp, ResponseSendFundsOp,
};
use signature::signature_buff::create_response_signature_buffer;

use crate::types::create_pending_transaction;

use super::types::{McMutation, MutualCredit};

/// Processes outgoing funds for a token channel.
/// Used to batch as many funds as possible.
#[derive(Debug)]
pub struct OutgoingMc {
    mutual_credit: MutualCredit,
}

#[derive(Debug)]
pub enum QueueOperationError {
    // RemoteMaxDebtTooLarge,
    InvalidRoute,
    // PkPairNotInRoute,
    CreditsCalcOverflow,
    RequestAlreadyExists,
    RequestDoesNotExist,
    InvalidResponseSignature,
    InvalidSrcPlainLock,
    DestPaymentExceedsTotal,
}

// TODO: Remove unused hint here:
#[allow(unused)]
/// A wrapper over a token channel, accumulating operations to be sent as one transaction.
impl OutgoingMc {
    // TODO: Take MutualCredit instead of &MutualCredit?
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
            FriendTcOp::RequestSendFunds(request_send_funds) => {
                self.queue_request_send_funds(request_send_funds)
            }
            FriendTcOp::ResponseSendFunds(response_send_funds) => {
                self.queue_response_send_funds(response_send_funds)
            }
            FriendTcOp::CancelSendFunds(cancel_send_funds) => {
                self.queue_cancel_send_funds(cancel_send_funds)
            }
        }
    }

    fn queue_request_send_funds(
        &mut self,
        request_send_funds: RequestSendFundsOp,
    ) -> Result<Vec<McMutation>, QueueOperationError> {
        if !request_send_funds.route.is_part_valid() {
            return Err(QueueOperationError::InvalidRoute);
        }

        if request_send_funds.dest_payment > request_send_funds.total_dest_payment {
            return Err(QueueOperationError::DestPaymentExceedsTotal);
        }

        // Calculate amount of credits to freeze
        let own_freeze_credits = request_send_funds
            .dest_payment
            .checked_add(request_send_funds.left_fees)
            .ok_or(QueueOperationError::CreditsCalcOverflow)?;

        let balance = &self.mutual_credit.state().balance;

        // Make sure we can freeze the credits
        let new_local_pending_debt = balance
            .local_pending_debt
            .checked_add(own_freeze_credits)
            .ok_or(QueueOperationError::CreditsCalcOverflow)?;

        let p_local_requests = &self.mutual_credit.state().pending_transactions.local;

        // Make sure that we don't have this request as a pending request already:
        if p_local_requests.contains_key(&request_send_funds.request_id) {
            return Err(QueueOperationError::RequestAlreadyExists);
        }

        // Add pending transaction:
        let pending_transaction = create_pending_transaction(&request_send_funds);

        let mut mc_mutations = Vec::new();
        let mc_mutation = McMutation::InsertLocalPendingTransaction(pending_transaction);
        self.mutual_credit.mutate(&mc_mutation);
        mc_mutations.push(mc_mutation);

        // If we are here, we can freeze the credits:
        let mc_mutation = McMutation::SetLocalPendingDebt(new_local_pending_debt);
        self.mutual_credit.mutate(&mc_mutation);
        mc_mutations.push(mc_mutation);

        Ok(mc_mutations)
    }

    fn queue_response_send_funds(
        &mut self,
        response_send_funds: ResponseSendFundsOp,
    ) -> Result<Vec<McMutation>, QueueOperationError> {
        // Make sure that id exists in remote_pending hashmap,
        // and access saved request details.
        let remote_pending_transactions = &self.mutual_credit.state().pending_transactions.remote;

        // Obtain pending request:
        let pending_transaction = remote_pending_transactions
            .get(&response_send_funds.request_id)
            .ok_or(QueueOperationError::RequestDoesNotExist)?
            .clone();
        // TODO: Possibly get rid of clone() here for optimization later

        // Verify src_plain_lock and dest_plain_lock:
        if response_send_funds.src_plain_lock.hash_lock() != pending_transaction.src_hashed_lock {
            return Err(QueueOperationError::InvalidSrcPlainLock);
        }

        // verify signature:
        let response_signature_buffer = create_response_signature_buffer(
            &self.mutual_credit.state().currency,
            response_send_funds.clone(),
            &pending_transaction,
        );
        // The response was signed by the destination node:
        let dest_public_key = if pending_transaction.route.public_keys.is_empty() {
            &self.mutual_credit.state().idents.local_public_key
        } else {
            pending_transaction.route.public_keys.last().unwrap()
        };

        // Verify response funds signature:
        if !verify_signature(
            &response_signature_buffer,
            dest_public_key,
            &response_send_funds.signature,
        ) {
            return Err(QueueOperationError::InvalidResponseSignature);
        }

        // Calculate amount of credits that were frozen:
        let freeze_credits = pending_transaction
            .dest_payment
            .checked_add(pending_transaction.left_fees)
            .unwrap();

        // Remove entry from remote_pending hashmap:
        let mut mc_mutations = Vec::new();
        let mc_mutation =
            McMutation::RemoveRemotePendingTransaction(response_send_funds.request_id);
        self.mutual_credit.mutate(&mc_mutation);
        mc_mutations.push(mc_mutation);

        // Decrease frozen credits and increase balance:
        let new_remote_pending_debt = self
            .mutual_credit
            .state()
            .balance
            .remote_pending_debt
            .checked_sub(freeze_credits)
            .unwrap();
        // Above unwrap() should never fail. This was already checked when a request message was
        // received.

        let mc_mutation = McMutation::SetRemotePendingDebt(new_remote_pending_debt);
        self.mutual_credit.mutate(&mc_mutation);
        mc_mutations.push(mc_mutation);

        let new_balance = self
            .mutual_credit
            .state()
            .balance
            .balance
            .checked_add_unsigned(freeze_credits)
            .unwrap();
        // Above unwrap() should never fail. This was already checked when a request message was
        // received.

        let mc_mutation = McMutation::SetBalance(new_balance);
        self.mutual_credit.mutate(&mc_mutation);
        mc_mutations.push(mc_mutation);

        Ok(mc_mutations)
    }

    fn queue_cancel_send_funds(
        &mut self,
        cancel_send_funds: CancelSendFundsOp,
    ) -> Result<Vec<McMutation>, QueueOperationError> {
        // Make sure that id exists in remote_pending hashmap,
        // and access saved request details.
        let remote_pending_transactions = &self.mutual_credit.state().pending_transactions.remote;

        // Obtain pending request:
        let pending_transaction = remote_pending_transactions
            .get(&cancel_send_funds.request_id)
            .ok_or(QueueOperationError::RequestDoesNotExist)?;

        let freeze_credits = pending_transaction
            .dest_payment
            .checked_add(pending_transaction.left_fees)
            .unwrap();

        // Remove entry from remote hashmap:
        let mut mc_mutations = Vec::new();

        let mc_mutation = McMutation::RemoveRemotePendingTransaction(cancel_send_funds.request_id);
        self.mutual_credit.mutate(&mc_mutation);
        mc_mutations.push(mc_mutation);

        // Decrease frozen credits:
        let new_remote_pending_debt = self
            .mutual_credit
            .state()
            .balance
            .remote_pending_debt
            .checked_sub(freeze_credits)
            .unwrap();

        let mc_mutation = McMutation::SetRemotePendingDebt(new_remote_pending_debt);
        self.mutual_credit.mutate(&mc_mutation);
        mc_mutations.push(mc_mutation);

        Ok(mc_mutations)
    }
}
