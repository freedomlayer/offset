use derive_more::From;

use crypto::hash_lock::HashLock;
use crypto::identity::verify_signature;

use common::async_rpc::OpError;
use common::safe_arithmetic::SafeSignedArithmetic;

use proto::crypto::PublicKey;
use proto::funder::messages::Currency;

use signature::signature_buff::create_response_signature_buffer;

use crate::route::Route;

use crate::mutual_credit::types::{
    McCancel, McDbClient, McOp, McRequest, McResponse, PendingTransaction,
};
use crate::mutual_credit::utils::{mc_response_signature_buffer, response_op_from_mc_response};

// TODO: Possibly rename to something shorter.
// TODO: Do we ever need the `pending_transaction` part for anything?
// If not, maybe we can remove it?
#[derive(Debug)]
pub struct IncomingResponseSendFundsOp {
    // TODO: Possibly rename to pending_request?
    pub pending_transaction: PendingTransaction,
    pub incoming_response: McResponse,
}

#[derive(Debug)]
pub struct IncomingCancelSendFundsOp {
    // TODO: Possibly rename to pending_request?
    pub pending_transaction: PendingTransaction,
    // TODO: incoming_cancel looks redundant, because we can already deduce request_id from
    // `pending_transaction`. Maybe remove it?
    pub incoming_cancel: McCancel,
}

#[derive(Debug)]
pub enum IncomingMessage {
    Request(McRequest),
    RequestCancel(McRequest),
    Response(IncomingResponseSendFundsOp),
    Cancel(IncomingCancelSendFundsOp),
}

#[derive(Debug)]
pub enum ProcessOutput {
    Incoming(IncomingMessage),
    Inconsistency,
}

#[derive(Debug, From)]
pub enum ProcessOperationError {
    /// The Route contains some public key twice.
    // InvalidRoute,
    // CreditsCalcOverflow,
    // RequestAlreadyExists,
    // RequestDoesNotExist,
    // InvalidResponseSignature,
    // InvalidSrcPlainLock,
    InvalidState,
    // DestPaymentExceedsTotal,
    OpError(OpError),
}

pub async fn process_operation(
    mc_client: &mut impl McDbClient,
    mc_op: McOp,
    currency: &Currency,
    remote_public_key: &PublicKey,
    remote_max_debt: u128,
) -> Result<ProcessOutput, ProcessOperationError> {
    match mc_op {
        McOp::Request(request) => {
            process_request_send_funds(mc_client, request, currency, remote_max_debt).await
        }
        McOp::Response(response) => {
            process_response_send_funds(mc_client, response, currency, remote_public_key).await
        }
        McOp::Cancel(cancel) => process_cancel_send_funds(mc_client, cancel).await,
    }
}

/// Process an incoming RequestSendFundsOp
async fn process_request_send_funds(
    mc_client: &mut impl McDbClient,
    request: McRequest,
    currency: &Currency,
    remote_max_debt: u128,
) -> Result<ProcessOutput, ProcessOperationError> {
    // Make sure that we don't have this request as a pending request already:
    if mc_client
        .get_remote_pending_transaction(request.request_id.clone())
        .await?
        .is_some()
    {
        // Request already exists
        return Ok(ProcessOutput::Inconsistency);
    }

    if !request.route.is_part_valid() {
        // Invalid route. We cancel the request
        return Ok(ProcessOutput::Incoming(IncomingMessage::RequestCancel(
            request,
        )));
    }

    // Calculate amount of credits to freeze
    let own_freeze_credits =
        if let Some(own_freeze_credits) = request.dest_payment.checked_add(request.left_fees) {
            own_freeze_credits
        } else {
            // Credits calculation overflow. We cancel the request:
            return Ok(ProcessOutput::Incoming(IncomingMessage::RequestCancel(
                request,
            )));
        };

    // Make sure we can freeze the credits
    let mc_balance = mc_client.get_balance().await?;

    let new_remote_pending_debt = if let Some(new_remote_pending_debt) = mc_balance
        .remote_pending_debt
        .checked_add(own_freeze_credits)
    {
        new_remote_pending_debt
    } else {
        // Credits calculation overflow. We cancel the request:
        return Ok(ProcessOutput::Incoming(IncomingMessage::RequestCancel(
            request,
        )));
    };

    // Check that balance + remote_pending_debt - remote_max_debt > 0:
    let add = if let Some(add) = mc_balance
        .balance
        .checked_add_unsigned(new_remote_pending_debt)
    {
        add
    } else {
        // Credits calculation overflow. We cancel the request:
        return Ok(ProcessOutput::Incoming(IncomingMessage::RequestCancel(
            request,
        )));
    };

    let incoming_message = if add.saturating_sub_unsigned(remote_max_debt) > 0 {
        // Insufficient trust:
        IncomingMessage::RequestCancel(request.clone())
    } else {
        IncomingMessage::Request(request.clone())
    };

    // Add pending transaction:
    let pending_transaction = PendingTransaction::from(request.clone());

    mc_client
        .insert_remote_pending_transaction(pending_transaction)
        .await?;
    // If we are here, we can freeze the credits:
    mc_client
        .set_remote_pending_debt(new_remote_pending_debt)
        .await?;

    Ok(ProcessOutput::Incoming(incoming_message))
}

async fn process_response_send_funds(
    mc_client: &mut impl McDbClient,
    response: McResponse,
    currency: &Currency,
    remote_public_key: &PublicKey,
) -> Result<ProcessOutput, ProcessOperationError> {
    // Make sure that id exists in local_pending hashmap,
    // and access saved request details.

    // Obtain pending request:
    let pending_transaction = if let Some(pending_transaction) = mc_client
        .get_local_pending_transaction(response.request_id.clone())
        .await?
    {
        pending_transaction
    } else {
        // Request does not exist
        return Ok(ProcessOutput::Inconsistency);
    };

    // Verify src_plain_lock and dest_plain_lock:
    if response.src_plain_lock.hash_lock() != pending_transaction.src_hashed_lock {
        // Invalid src plain lock
        return Ok(ProcessOutput::Inconsistency);
    }

    let dest_public_key = if pending_transaction.route.is_empty() {
        remote_public_key
    } else {
        pending_transaction
            .route
            .last()
            .ok_or(ProcessOperationError::InvalidState)?
    };

    let response_signature_buffer =
        mc_response_signature_buffer(&currency, &response, &pending_transaction);

    // Verify response funds signature:
    if !verify_signature(
        &response_signature_buffer,
        dest_public_key,
        &response.signature,
    ) {
        // Invalid response signature
        return Ok(ProcessOutput::Inconsistency);
    }

    // Calculate amount of credits that were frozen:
    let freeze_credits = pending_transaction
        .dest_payment
        .checked_add(pending_transaction.left_fees)
        .ok_or(ProcessOperationError::InvalidState)?;
    // Note: The above should never fail, because this was already checked during the
    // request message processing.

    // Remove entry from local_pending hashmap:
    mc_client
        .remove_local_pending_transaction(response.request_id.clone())
        .await?;

    // Decrease frozen credits:
    let mc_balance = mc_client.get_balance().await?;
    let new_local_pending_debt = mc_balance
        .local_pending_debt
        .checked_sub(freeze_credits)
        .ok_or(ProcessOperationError::InvalidState)?;
    mc_client
        .set_local_pending_debt(new_local_pending_debt)
        .await?;

    // Update out_fees:
    mc_client
        .set_out_fees(
            mc_balance
                .out_fees
                .checked_add(pending_transaction.left_fees.into())
                .ok_or(ProcessOperationError::InvalidState)?,
        )
        .await?;

    // Decrease balance:
    let mc_balance = mc_client.get_balance().await?;
    let new_balance = mc_balance
        .balance
        .checked_sub_unsigned(freeze_credits)
        .ok_or(ProcessOperationError::InvalidState)?;
    mc_client.set_balance(new_balance).await?;

    Ok(ProcessOutput::Incoming(IncomingMessage::Response(
        IncomingResponseSendFundsOp {
            pending_transaction,
            incoming_response: response,
        },
    )))
}

async fn process_cancel_send_funds(
    mc_client: &mut impl McDbClient,
    cancel: McCancel,
) -> Result<ProcessOutput, ProcessOperationError> {
    // Make sure that id exists in local_pending hashmap,
    // and access saved request details.

    // Obtain pending request:
    let pending_transaction = if let Some(pending_transaction) = mc_client
        .get_local_pending_transaction(cancel.request_id.clone())
        .await?
    {
        pending_transaction
    } else {
        // Request does not exist
        return Ok(ProcessOutput::Inconsistency);
    };

    mc_client
        .remove_local_pending_transaction(cancel.request_id.clone())
        .await?;

    let freeze_credits = pending_transaction
        .dest_payment
        .checked_add(pending_transaction.left_fees)
        .ok_or(ProcessOperationError::InvalidState)?;

    let mc_balance = mc_client.get_balance().await?;
    // Decrease frozen credits:
    let new_local_pending_debt = mc_balance
        .local_pending_debt
        .checked_sub(freeze_credits)
        .ok_or(ProcessOperationError::InvalidState)?;

    mc_client
        .set_local_pending_debt(new_local_pending_debt)
        .await?;

    // Return cancel_send_funds:
    Ok(ProcessOutput::Incoming(IncomingMessage::Cancel(
        IncomingCancelSendFundsOp {
            pending_transaction,
            incoming_cancel: cancel,
        },
    )))
}
