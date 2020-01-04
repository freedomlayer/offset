use crypto::hash_lock::HashLock;
use crypto::identity::verify_signature;

use common::safe_arithmetic::SafeSignedArithmetic;

use proto::funder::messages::{
    CancelSendFundsOp, CollectSendFundsOp, FriendTcOp, PendingTransaction, RequestSendFundsOp,
    ResponseSendFundsOp, TransactionStage,
};
use signature::signature_buff::create_response_signature_buffer;

use crate::types::create_pending_transaction;

use super::types::{McMutation, MutualCredit};

#[derive(Debug)]
pub struct IncomingResponseSendFundsOp {
    pub pending_transaction: PendingTransaction,
    pub incoming_response: ResponseSendFundsOp,
}

#[derive(Debug)]
pub struct IncomingCancelSendFundsOp {
    pub pending_transaction: PendingTransaction,
    pub incoming_cancel: CancelSendFundsOp,
}

#[derive(Debug)]
pub struct IncomingCollectSendFundsOp {
    pub pending_transaction: PendingTransaction,
    pub incoming_collect: CollectSendFundsOp,
}

#[derive(Debug)]
pub enum IncomingMessage {
    Request(RequestSendFundsOp),
    RequestCancel(RequestSendFundsOp),
    Response(IncomingResponseSendFundsOp),
    Cancel(IncomingCancelSendFundsOp),
    Collect(IncomingCollectSendFundsOp),
}

/// Resulting tasks to perform after processing an incoming operation.
pub struct ProcessOperationOutput {
    pub incoming_message: Option<IncomingMessage>,
    pub mc_mutations: Vec<McMutation>,
}

#[derive(Debug)]
pub enum ProcessOperationError {
    /// The Route contains some public key twice.
    InvalidRoute,
    CreditsCalcOverflow,
    RequestAlreadyExists,
    RequestDoesNotExist,
    InvalidResponseSignature,
    NotExpectingResponse,
    InvalidSrcPlainLock,
    InvalidDestPlainLock,
    NotExpectingCollect,
    DestPaymentExceedsTotal,
}

#[derive(Debug)]
pub struct ProcessTransListError {
    index: usize,
    process_trans_error: ProcessOperationError,
}

pub fn process_operations_list(
    mutual_credit: &mut MutualCredit,
    operations: Vec<FriendTcOp>,
    remote_max_debt: u128,
) -> Result<Vec<ProcessOperationOutput>, ProcessTransListError> {
    let mut outputs = Vec::new();

    // We do not change the original MutualCredit.
    // Instead, we are operating over a clone:
    // This operation is not very expensive, because we are using immutable data structures
    // (specifically, HashMaps).

    for (index, friend_tc_op) in operations.into_iter().enumerate() {
        match process_operation(mutual_credit, friend_tc_op, remote_max_debt) {
            Err(e) => {
                return Err(ProcessTransListError {
                    index,
                    process_trans_error: e,
                })
            }
            Ok(trans_output) => outputs.push(trans_output),
        }
    }
    Ok(outputs)
}

pub fn process_operation(
    mutual_credit: &mut MutualCredit,
    friend_tc_op: FriendTcOp,
    remote_max_debt: u128,
) -> Result<ProcessOperationOutput, ProcessOperationError> {
    match friend_tc_op {
        FriendTcOp::RequestSendFunds(request_send_funds) => {
            process_request_send_funds(mutual_credit, request_send_funds, remote_max_debt)
        }
        FriendTcOp::ResponseSendFunds(response_send_funds) => {
            process_response_send_funds(mutual_credit, response_send_funds)
        }
        FriendTcOp::CancelSendFunds(cancel_send_funds) => {
            process_cancel_send_funds(mutual_credit, cancel_send_funds)
        }
        FriendTcOp::CollectSendFunds(collect_send_funds) => {
            process_collect_send_funds(mutual_credit, collect_send_funds)
        }
    }
}

/// Process an incoming RequestSendFundsOp
fn process_request_send_funds(
    mutual_credit: &mut MutualCredit,
    request_send_funds: RequestSendFundsOp,
    remote_max_debt: u128,
) -> Result<ProcessOperationOutput, ProcessOperationError> {
    if !request_send_funds.route.is_part_valid() {
        return Err(ProcessOperationError::InvalidRoute);
    }

    if request_send_funds.dest_payment > request_send_funds.total_dest_payment {
        return Err(ProcessOperationError::DestPaymentExceedsTotal);
    }

    /*
    // Make sure that we are open to requests:
    if !mutual_credit.state().requests_status.local.is_open() {
        return Err(ProcessOperationError::LocalRequestsClosed);
    }
    */

    // Make sure that we don't have this request as a pending request already:
    let p_remote_requests = &mutual_credit.state().pending_transactions.remote;
    if p_remote_requests.contains_key(&request_send_funds.request_id) {
        return Err(ProcessOperationError::RequestAlreadyExists);
    }

    // Calculate amount of credits to freeze
    let own_freeze_credits = request_send_funds
        .dest_payment
        .checked_add(request_send_funds.left_fees)
        .ok_or(ProcessOperationError::CreditsCalcOverflow)?;

    // Make sure we can freeze the credits
    let balance = &mutual_credit.state().balance;

    let new_remote_pending_debt = balance
        .remote_pending_debt
        .checked_add(own_freeze_credits)
        .ok_or(ProcessOperationError::CreditsCalcOverflow)?;

    // Check that local_pending_debt - balance <= local_max_debt:
    let add = balance
        .balance
        .checked_add_unsigned(new_remote_pending_debt)
        .ok_or(ProcessOperationError::CreditsCalcOverflow)?;

    let incoming_message = if add
        .checked_sub_unsigned(remote_max_debt)
        .ok_or(ProcessOperationError::CreditsCalcOverflow)?
        > 0
    {
        // Insufficient trust:
        IncomingMessage::RequestCancel(request_send_funds.clone())
    } else {
        IncomingMessage::Request(request_send_funds.clone())
    };

    // Add pending transaction:
    let pending_transaction = create_pending_transaction(&request_send_funds);

    // let pending_friend_request = create_pending_transaction(&request_send_funds);

    let mut op_output = ProcessOperationOutput {
        incoming_message: Some(incoming_message),
        mc_mutations: Vec::new(),
    };

    let mc_mutation = McMutation::InsertRemotePendingTransaction(pending_transaction);
    mutual_credit.mutate(&mc_mutation);
    op_output.mc_mutations.push(mc_mutation);

    // If we are here, we can freeze the credits:
    let mc_mutation = McMutation::SetRemotePendingDebt(new_remote_pending_debt);
    mutual_credit.mutate(&mc_mutation);
    op_output.mc_mutations.push(mc_mutation);

    Ok(op_output)
}

fn process_response_send_funds(
    mutual_credit: &mut MutualCredit,
    response_send_funds: ResponseSendFundsOp,
) -> Result<ProcessOperationOutput, ProcessOperationError> {
    // Make sure that id exists in local_pending hashmap,
    // and access saved request details.
    let local_pending_transactions = &mutual_credit.state().pending_transactions.local;

    // Obtain pending request:
    // TODO: Possibly get rid of clone() here for optimization later
    let pending_transaction = local_pending_transactions
        .get(&response_send_funds.request_id)
        .ok_or(ProcessOperationError::RequestDoesNotExist)?
        .clone();

    let dest_public_key = if pending_transaction.route.public_keys.is_empty() {
        &mutual_credit.state().idents.remote_public_key
    } else {
        pending_transaction.route.public_keys.last().unwrap()
    };

    let response_signature_buffer = create_response_signature_buffer(
        &mutual_credit.state().currency,
        response_send_funds.clone(),
        &pending_transaction,
    );

    // Verify response funds signature:
    if !verify_signature(
        &response_signature_buffer,
        dest_public_key,
        &response_send_funds.signature,
    ) {
        return Err(ProcessOperationError::InvalidResponseSignature);
    }

    // We expect that the current stage is Request:
    if let TransactionStage::Request = pending_transaction.stage {
    } else {
        return Err(ProcessOperationError::NotExpectingResponse);
    }

    let mut mc_mutations = Vec::new();

    // Set the stage to Response, and remember dest_hashed_lock:
    let mc_mutation = McMutation::SetLocalPendingTransactionStage((
        response_send_funds.request_id.clone(),
        TransactionStage::Response(
            response_send_funds.dest_hashed_lock.clone(),
            response_send_funds.is_complete,
        ),
    ));
    mutual_credit.mutate(&mc_mutation);
    mc_mutations.push(mc_mutation);

    let incoming_message = Some(IncomingMessage::Response(IncomingResponseSendFundsOp {
        pending_transaction,
        incoming_response: response_send_funds,
    }));

    Ok(ProcessOperationOutput {
        incoming_message,
        mc_mutations,
    })
}

fn process_cancel_send_funds(
    mutual_credit: &mut MutualCredit,
    cancel_send_funds: CancelSendFundsOp,
) -> Result<ProcessOperationOutput, ProcessOperationError> {
    // Make sure that id exists in local_pending hashmap,
    // and access saved request details.
    let local_pending_transactions = &mutual_credit.state().pending_transactions.local;

    // Obtain pending request:
    let pending_transaction = local_pending_transactions
        .get(&cancel_send_funds.request_id)
        .ok_or(ProcessOperationError::RequestDoesNotExist)?
        .clone();
    // TODO: Possibly get rid of clone() here for optimization later

    let mut mc_mutations = Vec::new();

    // Remove entry from local_pending hashmap:
    let mc_mutation =
        McMutation::RemoveLocalPendingTransaction(cancel_send_funds.request_id.clone());
    mutual_credit.mutate(&mc_mutation);
    mc_mutations.push(mc_mutation);

    let freeze_credits = pending_transaction
        .dest_payment
        .checked_add(pending_transaction.left_fees)
        .unwrap();

    // Decrease frozen credits:
    let new_local_pending_debt = mutual_credit
        .state()
        .balance
        .local_pending_debt
        .checked_sub(freeze_credits)
        .unwrap();

    let mc_mutation = McMutation::SetLocalPendingDebt(new_local_pending_debt);
    mutual_credit.mutate(&mc_mutation);
    mc_mutations.push(mc_mutation);

    // Return cancel_send_funds:
    let incoming_message = Some(IncomingMessage::Cancel(IncomingCancelSendFundsOp {
        pending_transaction,
        incoming_cancel: cancel_send_funds,
    }));

    Ok(ProcessOperationOutput {
        incoming_message,
        mc_mutations,
    })
}

fn process_collect_send_funds(
    mutual_credit: &mut MutualCredit,
    collect_send_funds: CollectSendFundsOp,
) -> Result<ProcessOperationOutput, ProcessOperationError> {
    // Make sure that id exists in local_pending hashmap,
    // and access saved request details.
    let local_pending_transactions = &mutual_credit.state().pending_transactions.local;

    // Obtain pending request:
    // TODO: Possibly get rid of clone() here for optimization later
    let pending_transaction = local_pending_transactions
        .get(&collect_send_funds.request_id)
        .ok_or(ProcessOperationError::RequestDoesNotExist)?
        .clone();

    let dest_hashed_lock = match &pending_transaction.stage {
        TransactionStage::Response(dest_hashed_lock, _is_complete) => dest_hashed_lock,
        _ => return Err(ProcessOperationError::NotExpectingCollect),
    };

    // Verify src_plain_lock and dest_plain_lock:
    if collect_send_funds.src_plain_lock.hash_lock() != pending_transaction.src_hashed_lock {
        return Err(ProcessOperationError::InvalidSrcPlainLock);
    }

    if collect_send_funds.dest_plain_lock.hash_lock() != *dest_hashed_lock {
        return Err(ProcessOperationError::InvalidDestPlainLock);
    }

    // Calculate amount of credits that were frozen:
    let freeze_credits = pending_transaction
        .dest_payment
        .checked_add(pending_transaction.left_fees)
        .unwrap();
    // Note: The unwrap() above should never fail, because this was already checked during the
    // request message processing.

    let mut mc_mutations = Vec::new();

    // Remove entry from local_pending hashmap:
    let mc_mutation =
        McMutation::RemoveLocalPendingTransaction(collect_send_funds.request_id.clone());
    mutual_credit.mutate(&mc_mutation);
    mc_mutations.push(mc_mutation);

    // Decrease frozen credits and decrease balance:
    let new_local_pending_debt = mutual_credit
        .state()
        .balance
        .local_pending_debt
        .checked_sub(freeze_credits)
        .unwrap();

    let mc_mutation = McMutation::SetLocalPendingDebt(new_local_pending_debt);
    mutual_credit.mutate(&mc_mutation);
    mc_mutations.push(mc_mutation);

    let new_balance = mutual_credit
        .state()
        .balance
        .balance
        .checked_sub_unsigned(freeze_credits)
        .unwrap();

    let mc_mutation = McMutation::SetBalance(new_balance);
    mutual_credit.mutate(&mc_mutation);
    mc_mutations.push(mc_mutation);

    let incoming_message = Some(IncomingMessage::Collect(IncomingCollectSendFundsOp {
        pending_transaction,
        incoming_collect: collect_send_funds,
    }));

    Ok(ProcessOperationOutput {
        incoming_message,
        mc_mutations,
    })
}
