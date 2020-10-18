use derive_more::From;

use crypto::hash_lock::HashLock;
use crypto::identity::verify_signature;

use common::async_rpc::OpError;
use common::safe_arithmetic::SafeSignedArithmetic;

use proto::crypto::PublicKey;
use proto::funder::messages::{
    CancelSendFundsOp, Currency, FriendTcOp, RequestSendFundsOp, ResponseSendFundsOp,
};
use signature::signature_buff::create_response_signature_buffer;

use crate::mutual_credit::types::McTransaction;
use crate::types::create_pending_transaction;

#[derive(Debug, From)]
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
    OpError(OpError),
}

/// A wrapper over a token channel, accumulating operations to be sent as one transaction.
// TODO: Remove later:
#[allow(unused)]
pub async fn queue_operation(
    mc_transaction: &mut impl McTransaction,
    operation: FriendTcOp,
    currency: &Currency,
    local_public_key: &PublicKey,
) -> Result<(), QueueOperationError> {
    // TODO: Maybe remove clone from here later:
    match operation {
        FriendTcOp::RequestSendFunds(request_send_funds) => {
            queue_request_send_funds(mc_transaction, request_send_funds).await
        }
        FriendTcOp::ResponseSendFunds(response_send_funds) => {
            queue_response_send_funds(
                mc_transaction,
                response_send_funds,
                currency,
                local_public_key,
            )
            .await
        }
        FriendTcOp::CancelSendFunds(cancel_send_funds) => {
            queue_cancel_send_funds(mc_transaction, cancel_send_funds).await
        }
    }
}

async fn queue_request_send_funds(
    mc_transaction: &mut impl McTransaction,
    request_send_funds: RequestSendFundsOp,
) -> Result<(), QueueOperationError> {
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

    let mc_balance = mc_transaction.get_balance().await?;

    // Make sure we can freeze the credits
    let new_local_pending_debt = mc_balance
        .local_pending_debt
        .checked_add(own_freeze_credits)
        .ok_or(QueueOperationError::CreditsCalcOverflow)?;

    // Make sure that we don't have this request as a pending request already:
    if mc_transaction
        .get_local_pending_transaction(request_send_funds.request_id.clone())
        .await?
        .is_some()
    {
        return Err(QueueOperationError::RequestAlreadyExists);
    }

    // Add pending transaction:
    let pending_transaction = create_pending_transaction(&request_send_funds);
    mc_transaction
        .insert_local_pending_transaction(pending_transaction)
        .await?;

    // If we are here, we can freeze the credits:
    mc_transaction
        .set_local_pending_debt(new_local_pending_debt)
        .await?;

    Ok(())
}

async fn queue_response_send_funds(
    mc_transaction: &mut impl McTransaction,
    response_send_funds: ResponseSendFundsOp,
    currency: &Currency,
    local_public_key: &PublicKey,
) -> Result<(), QueueOperationError> {
    // Make sure that id exists in remote_pending hashmap,
    // and access saved request details.

    // Obtain pending request:
    let pending_transaction = mc_transaction
        .get_remote_pending_transaction(response_send_funds.request_id.clone())
        .await?
        .ok_or(QueueOperationError::RequestDoesNotExist)?;
    // TODO: Possibly get rid of clone() here for optimization later

    // Verify src_plain_lock and dest_plain_lock:
    if response_send_funds.src_plain_lock.hash_lock() != pending_transaction.src_hashed_lock {
        return Err(QueueOperationError::InvalidSrcPlainLock);
    }

    // verify signature:
    let response_signature_buffer = create_response_signature_buffer(
        currency,
        response_send_funds.clone(),
        &pending_transaction,
    );
    // The response was signed by the destination node:
    let dest_public_key = if pending_transaction.route.public_keys.is_empty() {
        local_public_key
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
    mc_transaction
        .remove_remote_pending_transaction(response_send_funds.request_id)
        .await?;

    // Decrease frozen credits
    let mc_balance = mc_transaction.get_balance().await?;
    let new_remote_pending_debt = mc_balance
        .remote_pending_debt
        .checked_sub(freeze_credits)
        .unwrap();
    // Above unwrap() should never fail. This was already checked when a request message was
    // received.

    mc_transaction
        .set_remote_pending_debt(new_remote_pending_debt)
        .await?;

    // Increase balance:
    let mc_balance = mc_transaction.get_balance().await?;
    let new_balance = mc_balance
        .balance
        .checked_add_unsigned(freeze_credits)
        .unwrap();
    // Above unwrap() should never fail. This was already checked when a request message was
    // received.

    mc_transaction.set_balance(new_balance).await?;

    Ok(())
}

async fn queue_cancel_send_funds(
    mc_transaction: &mut impl McTransaction,
    cancel_send_funds: CancelSendFundsOp,
) -> Result<(), QueueOperationError> {
    // Make sure that id exists in remote_pending hashmap,
    // and access saved request details.

    // Obtain pending request:
    let pending_transaction = mc_transaction
        .get_remote_pending_transaction(cancel_send_funds.request_id.clone())
        .await?
        .ok_or(QueueOperationError::RequestDoesNotExist)?;

    let freeze_credits = pending_transaction
        .dest_payment
        .checked_add(pending_transaction.left_fees)
        .unwrap();

    // Remove entry from remote hashmap:
    mc_transaction
        .remove_remote_pending_transaction(cancel_send_funds.request_id.clone())
        .await?;

    // Decrease frozen credits:
    let mc_balance = mc_transaction.get_balance().await?;
    let new_remote_pending_debt = mc_balance
        .remote_pending_debt
        .checked_sub(freeze_credits)
        .unwrap();

    mc_transaction
        .set_remote_pending_debt(new_remote_pending_debt)
        .await?;

    Ok(())
}
