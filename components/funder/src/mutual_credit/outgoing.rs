use crypto::hash_lock::HashLock;
use crypto::identity::verify_signature;

use common::safe_arithmetic::SafeSignedArithmetic;

use proto::funder::messages::{
    CancelSendFundsOp, CollectSendFundsOp, FriendTcOp, RequestSendFundsOp, ResponseSendFundsOp,
    TransactionStage,
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
    RemoteMaxDebtTooLarge,
    InvalidRoute,
    PkPairNotInRoute,
    CreditsCalcOverflow,
    RequestAlreadyExists,
    RequestDoesNotExist,
    InvalidResponseSignature,
    NotExpectingResponse,
    NotExpectingCollect,
    InvalidSrcPlainLock,
    InvalidDestPlainLock,
    DestPaymentExceedsTotal,
}

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
            FriendTcOp::CollectSendFunds(collect_send_funds) => {
                self.queue_collect_send_funds(collect_send_funds)
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

        /*
        // Make sure that remote side is open to requests:
        if !self.mutual_credit.state().requests_status.remote.is_open() {
            return Err(QueueOperationError::RemoteRequestsClosed);
        }
        */

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

        // verify signature:
        let response_signature_buffer = create_response_signature_buffer(
            &self.mutual_credit.state().currency,
            &response_send_funds,
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

        // We expect that the current stage is Request:
        if let TransactionStage::Request = pending_transaction.stage {
        } else {
            return Err(QueueOperationError::NotExpectingResponse);
        }

        let mut mc_mutations = Vec::new();

        // Set the stage to Response, and remember dest_hashed_lock:
        let mc_mutation = McMutation::SetRemotePendingTransactionStage((
            response_send_funds.request_id,
            TransactionStage::Response(
                response_send_funds.dest_hashed_lock.clone(),
                response_send_funds.is_complete,
            ),
        ));
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

    fn queue_collect_send_funds(
        &mut self,
        collect_send_funds: CollectSendFundsOp,
    ) -> Result<Vec<McMutation>, QueueOperationError> {
        // Make sure that id exists in remote_pending hashmap,
        // and access saved request details.
        let remote_pending_transactions = &self.mutual_credit.state().pending_transactions.remote;

        // Obtain pending request:
        let pending_transaction = remote_pending_transactions
            .get(&collect_send_funds.request_id)
            .ok_or(QueueOperationError::RequestDoesNotExist)?
            .clone();
        // TODO: Possibly get rid of clone() here for optimization later

        let dest_hashed_lock = match &pending_transaction.stage {
            TransactionStage::Response(dest_hashed_lock, _is_complete) => dest_hashed_lock,
            _ => return Err(QueueOperationError::NotExpectingCollect),
        };

        // Verify src_plain_lock and dest_plain_lock:
        if collect_send_funds.src_plain_lock.hash_lock() != pending_transaction.src_hashed_lock {
            return Err(QueueOperationError::InvalidSrcPlainLock);
        }

        if collect_send_funds.dest_plain_lock.hash_lock() != *dest_hashed_lock {
            return Err(QueueOperationError::InvalidDestPlainLock);
        }

        // Calculate amount of credits that were frozen:
        let freeze_credits = pending_transaction
            .dest_payment
            .checked_add(pending_transaction.left_fees)
            .unwrap();

        // Remove entry from remote_pending hashmap:
        let mut mc_mutations = Vec::new();
        let mc_mutation = McMutation::RemoveRemotePendingTransaction(collect_send_funds.request_id);
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
}
