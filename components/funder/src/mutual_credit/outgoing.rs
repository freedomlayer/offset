use derive_more::From;

use crypto::hash_lock::HashLock;
use crypto::identity::verify_signature;

use common::async_rpc::OpError;
use common::safe_arithmetic::SafeSignedArithmetic;

use proto::crypto::PublicKey;
use proto::funder::messages::Currency;
use signature::signature_buff::create_response_signature_buffer;

use crate::route::Route;

use crate::mutual_credit::types::{McCancel, McDbClient, McOp, McRequest, McResponse};
use crate::mutual_credit::utils::{
    pending_transaction_from_mc_request, response_op_from_mc_response,
};

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
    // DestPaymentExceedsTotal,
    OpError(OpError),
}

/// A wrapper over a token channel, accumulating operations to be sent as one transaction.
pub async fn queue_operation(
    mc_client: &mut impl McDbClient,
    operation: McOp,
    currency: &Currency,
    local_public_key: &PublicKey,
) -> Result<(), QueueOperationError> {
    // TODO: Maybe remove clone from here later:
    match operation {
        McOp::Request(request) => queue_request_send_funds(mc_client, request, currency).await,
        McOp::Response(response) => {
            queue_response_send_funds(mc_client, response, currency, local_public_key).await
        }
        McOp::Cancel(cancel) => queue_cancel_send_funds(mc_client, cancel).await,
    }
}

async fn queue_request_send_funds(
    mc_client: &mut impl McDbClient,
    request: McRequest,
    currency: &Currency,
) -> Result<(), QueueOperationError> {
    if !request.route.is_part_valid() {
        return Err(QueueOperationError::InvalidRoute);
    }

    /*
    if request_send_funds.dest_payment > request_send_funds.total_dest_payment {
        return Err(QueueOperationError::DestPaymentExceedsTotal);
    }
    */

    // Calculate amount of credits to freeze
    let own_freeze_credits = request
        .dest_payment
        .checked_add(request.left_fees)
        .ok_or(QueueOperationError::CreditsCalcOverflow)?;

    let mc_balance = mc_client.get_balance().await?;

    // Make sure we can freeze the credits
    let new_local_pending_debt = mc_balance
        .local_pending_debt
        .checked_add(own_freeze_credits)
        .ok_or(QueueOperationError::CreditsCalcOverflow)?;

    // Make sure that we don't have this request as a pending request already:
    if mc_client
        .get_local_pending_transaction(request.request_id.clone())
        .await?
        .is_some()
    {
        return Err(QueueOperationError::RequestAlreadyExists);
    }

    // Add pending transaction:
    let pending_transaction =
        pending_transaction_from_mc_request(request.clone(), currency.clone());
    mc_client
        .insert_local_pending_transaction(pending_transaction)
        .await?;

    // If we are here, we can freeze the credits:
    mc_client
        .set_local_pending_debt(new_local_pending_debt)
        .await?;

    Ok(())
}

async fn queue_response_send_funds(
    mc_client: &mut impl McDbClient,
    response: McResponse,
    currency: &Currency,
    local_public_key: &PublicKey,
) -> Result<(), QueueOperationError> {
    // Make sure that id exists in remote_pending hashmap,
    // and access saved request details.

    // Obtain pending request:
    let pending_transaction = mc_client
        .get_remote_pending_transaction(response.request_id.clone())
        .await?
        .ok_or(QueueOperationError::RequestDoesNotExist)?;
    // TODO: Possibly get rid of clone() here for optimization later

    // Verify src_plain_lock and dest_plain_lock:
    if response.src_plain_lock.hash_lock() != pending_transaction.src_hashed_lock {
        return Err(QueueOperationError::InvalidSrcPlainLock);
    }

    // verify signature:
    let response_signature_buffer = create_response_signature_buffer(
        currency,
        response_op_from_mc_response(response.clone()),
        &pending_transaction,
    );
    // The response was signed by the destination node:
    let dest_public_key = if pending_transaction.route.is_empty() {
        local_public_key
    } else {
        pending_transaction.route.last().unwrap()
    };

    // Verify response funds signature:
    if !verify_signature(
        &response_signature_buffer,
        dest_public_key,
        &response.signature,
    ) {
        return Err(QueueOperationError::InvalidResponseSignature);
    }

    // Calculate amount of credits that were frozen:
    let freeze_credits = pending_transaction
        .dest_payment
        .checked_add(pending_transaction.left_fees)
        .unwrap();

    // Remove entry from remote_pending hashmap:
    mc_client
        .remove_remote_pending_transaction(response.request_id)
        .await?;

    // Decrease frozen credits
    let mc_balance = mc_client.get_balance().await?;
    let new_remote_pending_debt = mc_balance
        .remote_pending_debt
        .checked_sub(freeze_credits)
        .unwrap();
    // Above unwrap() should never fail. This was already checked when a request message was
    // received.

    mc_client
        .set_remote_pending_debt(new_remote_pending_debt)
        .await?;

    // Update in_fees:
    mc_client
        .set_in_fees(
            mc_balance
                .in_fees
                .checked_add(pending_transaction.left_fees.into())
                .unwrap(),
        )
        .await?;

    // Increase balance:
    let mc_balance = mc_client.get_balance().await?;
    let new_balance = mc_balance
        .balance
        .checked_add_unsigned(freeze_credits)
        .unwrap();
    // Above unwrap() should never fail. This was already checked when a request message was
    // received.

    mc_client.set_balance(new_balance).await?;

    Ok(())
}

async fn queue_cancel_send_funds(
    mc_client: &mut impl McDbClient,
    cancel: McCancel,
) -> Result<(), QueueOperationError> {
    // Make sure that id exists in remote_pending hashmap,
    // and access saved request details.

    // Obtain pending request:
    let pending_transaction = mc_client
        .get_remote_pending_transaction(cancel.request_id.clone())
        .await?
        .ok_or(QueueOperationError::RequestDoesNotExist)?;

    let freeze_credits = pending_transaction
        .dest_payment
        .checked_add(pending_transaction.left_fees)
        .unwrap();

    // Remove entry from remote hashmap:
    mc_client
        .remove_remote_pending_transaction(cancel.request_id.clone())
        .await?;

    // Decrease frozen credits:
    let mc_balance = mc_client.get_balance().await?;
    let new_remote_pending_debt = mc_balance
        .remote_pending_debt
        .checked_sub(freeze_credits)
        .unwrap();

    mc_client
        .set_remote_pending_debt(new_remote_pending_debt)
        .await?;

    Ok(())
}
