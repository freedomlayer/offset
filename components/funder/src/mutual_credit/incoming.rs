use derive_more::From;

use crypto::hash_lock::HashLock;
use crypto::identity::verify_signature;

use common::async_rpc::OpError;
use common::safe_arithmetic::SafeSignedArithmetic;

use proto::crypto::PublicKey;
use proto::funder::messages::{
    CancelSendFundsOp, Currency, FriendTcOp, PendingTransaction, RequestSendFundsOp,
    ResponseSendFundsOp,
};

use signature::signature_buff::create_response_signature_buffer;

use crate::route::Route;
use crate::types::create_pending_transaction;

use super::types::McDbClient;

#[derive(Debug)]
pub struct IncomingResponseSendFundsOp {
    pub pending_transaction: PendingTransaction,
    pub incoming_response: ResponseSendFundsOp,
}

#[derive(Debug)]
pub struct IncomingCancelSendFundsOp {
    pub pending_transaction: PendingTransaction,
    // TODO: incoming_cancel looks redundant, because we can already deduce request_id from
    // `pending_transaction`. Maybe remove it?
    pub incoming_cancel: CancelSendFundsOp,
}

#[derive(Debug)]
pub enum IncomingMessage {
    Request(RequestSendFundsOp),
    RequestCancel(RequestSendFundsOp),
    Response(IncomingResponseSendFundsOp),
    Cancel(IncomingCancelSendFundsOp),
}

#[derive(Debug, From)]
pub enum ProcessOperationError {
    /// The Route contains some public key twice.
    InvalidRoute,
    CreditsCalcOverflow,
    RequestAlreadyExists,
    RequestDoesNotExist,
    InvalidResponseSignature,
    InvalidSrcPlainLock,
    DestPaymentExceedsTotal,
    OpError(OpError),
}

pub async fn process_operation(
    mc_client: &mut impl McDbClient,
    friend_tc_op: FriendTcOp,
    currency: &Currency,
    remote_public_key: &PublicKey,
    remote_max_debt: u128,
) -> Result<IncomingMessage, ProcessOperationError> {
    match friend_tc_op {
        FriendTcOp::RequestSendFunds(request_send_funds) => {
            process_request_send_funds(mc_client, request_send_funds, remote_max_debt).await
        }
        FriendTcOp::ResponseSendFunds(response_send_funds) => {
            process_response_send_funds(mc_client, response_send_funds, currency, remote_public_key)
                .await
        }
        FriendTcOp::CancelSendFunds(cancel_send_funds) => {
            process_cancel_send_funds(mc_client, cancel_send_funds).await
        }
    }
}

/// Process an incoming RequestSendFundsOp
async fn process_request_send_funds(
    mc_client: &mut impl McDbClient,
    request_send_funds: RequestSendFundsOp,
    remote_max_debt: u128,
) -> Result<IncomingMessage, ProcessOperationError> {
    if !request_send_funds.route.is_part_valid() {
        return Err(ProcessOperationError::InvalidRoute);
    }

    if request_send_funds.dest_payment > request_send_funds.total_dest_payment {
        return Err(ProcessOperationError::DestPaymentExceedsTotal);
    }

    // Make sure that we don't have this request as a pending request already:
    let opt_remote_pending_transaction = mc_client
        .get_remote_pending_transaction(request_send_funds.request_id.clone())
        .await?;
    if opt_remote_pending_transaction.is_some() {
        return Err(ProcessOperationError::RequestAlreadyExists);
    }

    // Calculate amount of credits to freeze
    let own_freeze_credits = request_send_funds
        .dest_payment
        .checked_add(request_send_funds.left_fees)
        .ok_or(ProcessOperationError::CreditsCalcOverflow)?;

    // Make sure we can freeze the credits
    let mc_balance = mc_client.get_balance().await?;

    let new_remote_pending_debt = mc_balance
        .remote_pending_debt
        .checked_add(own_freeze_credits)
        .ok_or(ProcessOperationError::CreditsCalcOverflow)?;

    // Check that local_pending_debt - balance <= local_max_debt:
    let add = mc_balance
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

    mc_client
        .insert_remote_pending_transaction(pending_transaction)
        .await?;
    // If we are here, we can freeze the credits:
    mc_client
        .set_remote_pending_debt(new_remote_pending_debt)
        .await?;

    Ok(incoming_message)
}

async fn process_response_send_funds(
    mc_client: &mut impl McDbClient,
    response_send_funds: ResponseSendFundsOp,
    currency: &Currency,
    remote_public_key: &PublicKey,
) -> Result<IncomingMessage, ProcessOperationError> {
    // Make sure that id exists in local_pending hashmap,
    // and access saved request details.

    // Obtain pending request:
    let pending_transaction = mc_client
        .get_local_pending_transaction(response_send_funds.request_id.clone())
        .await?
        .ok_or(ProcessOperationError::RequestDoesNotExist)?;

    // Verify src_plain_lock and dest_plain_lock:
    if response_send_funds.src_plain_lock.hash_lock() != pending_transaction.src_hashed_lock {
        return Err(ProcessOperationError::InvalidSrcPlainLock);
    }

    let dest_public_key = if pending_transaction.route.is_empty() {
        remote_public_key
    } else {
        pending_transaction.route.last().unwrap()
    };

    let response_signature_buffer = create_response_signature_buffer(
        currency,
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

    // Calculate amount of credits that were frozen:
    let freeze_credits = pending_transaction
        .dest_payment
        .checked_add(pending_transaction.left_fees)
        .unwrap();
    // Note: The unwrap() above should never fail, because this was already checked during the
    // request message processing.

    // Remove entry from local_pending hashmap:
    mc_client
        .remove_local_pending_transaction(response_send_funds.request_id.clone())
        .await?;

    // Decrease frozen credits:
    let mc_balance = mc_client.get_balance().await?;
    let new_local_pending_debt = mc_balance
        .local_pending_debt
        .checked_sub(freeze_credits)
        .unwrap();
    mc_client
        .set_local_pending_debt(new_local_pending_debt)
        .await?;

    // Update out_fees:
    mc_client
        .set_out_fees(
            mc_balance
                .out_fees
                .checked_add(pending_transaction.left_fees.into())
                .unwrap(),
        )
        .await?;

    // Decrease balance:
    let mc_balance = mc_client.get_balance().await?;
    let new_balance = mc_balance
        .balance
        .checked_sub_unsigned(freeze_credits)
        .unwrap();
    mc_client.set_balance(new_balance).await?;

    Ok(IncomingMessage::Response(IncomingResponseSendFundsOp {
        pending_transaction,
        incoming_response: response_send_funds,
    }))
}

async fn process_cancel_send_funds(
    mc_client: &mut impl McDbClient,
    cancel_send_funds: CancelSendFundsOp,
) -> Result<IncomingMessage, ProcessOperationError> {
    // Make sure that id exists in local_pending hashmap,
    // and access saved request details.

    // Obtain pending request:
    let pending_transaction = mc_client
        .get_local_pending_transaction(cancel_send_funds.request_id.clone())
        .await?
        .ok_or(ProcessOperationError::RequestDoesNotExist)?;

    mc_client
        .remove_local_pending_transaction(cancel_send_funds.request_id.clone())
        .await?;

    let freeze_credits = pending_transaction
        .dest_payment
        .checked_add(pending_transaction.left_fees)
        .unwrap();

    let mc_balance = mc_client.get_balance().await?;
    // Decrease frozen credits:
    let new_local_pending_debt = mc_balance
        .local_pending_debt
        .checked_sub(freeze_credits)
        .unwrap();

    mc_client
        .set_local_pending_debt(new_local_pending_debt)
        .await?;

    // Return cancel_send_funds:
    Ok(IncomingMessage::Cancel(IncomingCancelSendFundsOp {
        pending_transaction,
        incoming_cancel: cancel_send_funds,
    }))
}
