use std::collections::{HashMap, HashSet};

use futures::StreamExt;

use derive_more::From;

use common::async_rpc::OpError;
use common::safe_arithmetic::{SafeSignedArithmetic, SafeUnsignedArithmetic};

use database::transaction::Transaction;

use identity::IdentityClient;

use proto::app_server::messages::RelayAddressPort;
use proto::crypto::{NodePort, PublicKey, Uid};
use proto::funder::messages::{
    CancelSendFundsOp, Currency, FriendMessage, FriendTcOp, MoveToken, MoveTokenRequest,
    RelaysUpdate, RequestSendFundsOp, ResetTerms, ResponseSendFundsOp,
};
use proto::index_server::messages::{IndexMutation, RemoveFriendCurrency, UpdateFriendCurrency};
use proto::net::messages::NetAddress;

use crypto::rand::{CryptoRandom, RandGen};

use crate::route::Route;

use crate::mutual_credit::incoming::{
    IncomingCancelSendFundsOp, IncomingMessage, IncomingResponseSendFundsOp,
};
use crate::mutual_credit::{McCancel, McRequest};
use crate::router::types::{
    BackwardsOp, CurrencyInfo, RouterControl, RouterDbClient, RouterError, RouterInfo,
    RouterOutput, RouterState, SentRelay,
};
use crate::router::utils::move_token::handle_in_move_token_index_mutations;
use crate::token_channel::{ReceiveMoveTokenOutput, TcDbClient, TcStatus, TokenChannelError};

/// Check if credit line between this node and a friend is ready
/// Works for sending requests in both directions
async fn is_credit_line_ready(
    control: &mut impl RouterControl,
    friend_public_key: PublicKey,
    currency: Currency,
    request_id: Uid,
) -> Result<bool, RouterError> {
    // Friend must be online:
    if !control
        .access()
        .ephemeral
        .liveness
        .is_online(&friend_public_key)
    {
        return Ok(false);
    }

    // Currency must exist:
    let currency_info = if let Some(currency_info) = control
        .access()
        .router_db_client
        .get_currency_info(friend_public_key, currency)
        .await?
    {
        currency_info
    } else {
        return Ok(false);
    };

    // Note: We bypass `is_open` check if the request is of local origin:
    Ok(currency_info.is_open
        || control
            .access()
            .router_db_client
            .is_request_local_origin(request_id)
            .await?)
}

async fn incoming_message_request(
    control: &mut impl RouterControl,
    info: &RouterInfo,
    friend_public_key: PublicKey,
    currency: Currency,
    mut mc_request: McRequest,
) -> Result<(), RouterError> {
    // Make sure that we are ready to receive this request operation
    // We might not be ready in the case of currency being closed.
    if !is_credit_line_ready(
        control,
        friend_public_key.clone(),
        currency.clone(),
        mc_request.request_id.clone(),
    )
    .await?
    {
        // Return a cancel message and flush origin friend
        control
            .access()
            .router_db_client
            .pending_backwards_push_back(
                friend_public_key.clone(),
                BackwardsOp::Cancel(
                    currency.clone(),
                    McCancel {
                        request_id: mc_request.request_id.clone(),
                    },
                ),
            )
            .await?;

        control.access().send_commands.move_token(friend_public_key);

        return Ok(());
    }

    // If directed to us, punt
    if mc_request.route.is_empty() {
        // Directed to us. Punt:
        control
            .access()
            .output
            .add_incoming_request(currency.clone(), mc_request.into());
        return Ok(());
    }

    // - If directed to another friend:
    //      - If friend exists and ready, forward to next hop
    //      - Otherwise, queue cancel
    // Route is not empty. We need to forward the request to a friend
    let next_public_key = mc_request.route.remove(0);

    let is_credit_line_ready = is_credit_line_ready(
        control,
        next_public_key.clone(),
        currency.clone(),
        mc_request.request_id.clone(),
    )
    .await?;

    // Check if next public key corresponds to a friend that is ready
    let should_forward = is_credit_line_ready
        && !control
            .access()
            .router_db_client
            .is_local_request_exists(mc_request.request_id.clone())
            .await?;

    // Attempt to collect fees according to rate. If credits in fees jar are insufficient, do not
    // forward, and cancel the request.
    todo!();

    if should_forward {
        // Queue request to friend and flush destination friend
        control
            .access()
            .router_db_client
            .pending_requests_push_back(next_public_key.clone(), currency, mc_request.into())
            .await?;
        // TODO: flush_friend() is delicate. We might be able to aggregate some more pending
        // requests before flushing this friend. Find out how to do this.

        control.access().send_commands.move_token(next_public_key);
    } else {
        // Return a cancel message and flush origin friend
        control
            .access()
            .router_db_client
            .pending_backwards_push_back(
                friend_public_key.clone(),
                BackwardsOp::Cancel(
                    currency,
                    McCancel {
                        request_id: mc_request.request_id.clone(),
                    },
                ),
            )
            .await?;

        control.access().send_commands.move_token(friend_public_key);
    }

    Ok(())
}

/// An incoming request was received, but due to insufficient trust we can not
/// forward this request. We return a cancel message to the sender friend
async fn incoming_message_request_cancel(
    control: &mut impl RouterControl,
    friend_public_key: PublicKey,
    currency: Currency,
    mc_request: McRequest,
) -> Result<(), RouterError> {
    if control
        .access()
        .router_db_client
        .tc_db_client(friend_public_key.clone())
        .await?
        .ok_or(RouterError::InvalidState)?
        .get_tc_status()
        .await?
        .is_consistent()
    {
        let backwards_op = BackwardsOp::Cancel(
            currency,
            McCancel {
                request_id: mc_request.request_id,
            },
        );

        // Queue cancel to friend_public_key (Request origin)
        control
            .access()
            .router_db_client
            .pending_backwards_push_back(friend_public_key.clone(), backwards_op)
            .await?;
    } else {
        // We have just received a cancel message from this friend.
        // We expect that this friend is consistent
        return Err(RouterError::InvalidState);
    }
    Ok(())
}

async fn incoming_message_response(
    control: &mut impl RouterControl,
    info: &RouterInfo,
    friend_public_key: PublicKey,
    currency: Currency,
    incoming_response: IncomingResponseSendFundsOp,
) -> Result<(), RouterError> {
    // Gather information about request's origin:
    let opt_request_origin = control
        .access()
        .router_db_client
        .get_remote_pending_request_origin(incoming_response.incoming_response.request_id.clone())
        .await?;
    let is_local_origin = control
        .access()
        .router_db_client
        .is_request_local_origin(incoming_response.incoming_response.request_id.clone())
        .await?;

    match (is_local_origin, opt_request_origin) {
        (false, None) => {
            // Request was probably originated from a friend that became inconsistent.
            // We have nothing to do here.
        }
        (true, None) => {
            // Request has originated locally

            // Clear the request from local requests list
            control
                .access()
                .router_db_client
                .remove_local_request(incoming_response.incoming_response.request_id.clone())
                .await?;

            // We punt the response
            control
                .access()
                .output
                .add_incoming_response(currency.clone(), incoming_response.incoming_response);
        }
        (false, Some(request_origin)) => {
            // Request has originated from a friend.
            // We assume that friend must be consistent, otherwise it would have not been found as
            // an origin.

            // Push response:
            control
                .access()
                .router_db_client
                .pending_backwards_push_back(
                    friend_public_key.clone(),
                    BackwardsOp::Response(currency, incoming_response.incoming_response),
                )
                .await?;

            control.access().send_commands.move_token(friend_public_key);
        }
        (true, Some(request_origin)) => {
            // This means the request has both originated locally, and from a friend.
            // Note that this only happen during a cycle request, but a cycle always begins and
            // ends at the local node.
            // Therefore, this should never happen when processing a friend incoming response.
            return Err(RouterError::InvalidState);
        }
    }
    Ok(())
}

// TODO: This function seems too similar to `incoming_message_response`
async fn incoming_message_cancel(
    control: &mut impl RouterControl,
    info: &RouterInfo,
    friend_public_key: PublicKey,
    currency: Currency,
    incoming_cancel: IncomingCancelSendFundsOp,
) -> Result<(), RouterError> {
    // Gather information about request's origin:
    let opt_request_origin = control
        .access()
        .router_db_client
        .get_remote_pending_request_origin(incoming_cancel.incoming_cancel.request_id.clone())
        .await?;
    let is_local_origin = control
        .access()
        .router_db_client
        .is_request_local_origin(incoming_cancel.incoming_cancel.request_id.clone())
        .await?;

    match (is_local_origin, opt_request_origin) {
        (false, None) => {
            // Request was probably originated from a friend that became inconsistent.
            // We have nothing to do here.
        }
        (true, None) => {
            // Request has originated locally

            // Clear the request from local requests list
            control
                .access()
                .router_db_client
                .remove_local_request(incoming_cancel.incoming_cancel.request_id.clone())
                .await?;

            // We punt the cancel
            control
                .access()
                .output
                .add_incoming_cancel(currency, incoming_cancel.incoming_cancel);
        }
        (false, Some(request_origin)) => {
            // Request has originated from a friend.
            // We assume that friend must be consistent, otherwise it would have not been found as
            // an origin.

            // Push cancel:
            control
                .access()
                .router_db_client
                .pending_backwards_push_back(
                    friend_public_key.clone(),
                    BackwardsOp::Cancel(currency, incoming_cancel.incoming_cancel),
                )
                .await?;

            control
                .access()
                .send_commands
                .move_token(friend_public_key.clone());
        }
        (true, Some(request_origin)) => {
            // This means the request has both originated locally, and from a friend.
            // Note that this only happen during a cycle request, but a cycle always begins and
            // ends at the local node.
            // Therefore, this should never happen when processing a friend incoming cancel.
            return Err(RouterError::InvalidState);
        }
    }
    Ok(())
}

async fn incoming_move_token_request(
    control: &mut impl RouterControl,
    info: &RouterInfo,
    friend_public_key: PublicKey,
    in_move_token_request: MoveTokenRequest,
) -> Result<(), RouterError> {
    let (receive_move_token_output, index_mutations) = handle_in_move_token_index_mutations(
        control,
        info,
        in_move_token_request.move_token,
        friend_public_key.clone(),
    )
    .await?;

    // Add mutations:
    for index_mutation in index_mutations {
        control.access().output.add_index_mutation(index_mutation);
    }

    match receive_move_token_output {
        ReceiveMoveTokenOutput::Duplicate => {
            // We should have nothing else to send at this point, otherwise we would have already
            // sent it.
            control
                .access()
                .send_commands
                .move_token_allow_empty(friend_public_key.clone());
        }
        ReceiveMoveTokenOutput::RetransmitOutgoing(move_token) => {
            // Retransmit outgoing move token message to friend:
            control
                .access()
                .send_commands
                .move_token_allow_empty(friend_public_key.clone());
        }
        ReceiveMoveTokenOutput::Received(move_token_received) => {
            for (currency, incoming_message) in move_token_received.incoming_messages {
                match incoming_message {
                    IncomingMessage::Request(mc_request) => {
                        incoming_message_request(
                            control,
                            info,
                            friend_public_key.clone(),
                            currency,
                            mc_request,
                        )
                        .await
                    }
                    IncomingMessage::RequestCancel(mc_request) => {
                        incoming_message_request_cancel(
                            control,
                            friend_public_key.clone(),
                            currency,
                            mc_request,
                        )
                        .await
                    }
                    IncomingMessage::Response(incoming_response) => {
                        incoming_message_response(
                            control,
                            info,
                            friend_public_key.clone(),
                            currency,
                            incoming_response,
                        )
                        .await
                    }
                    IncomingMessage::Cancel(incoming_cancel) => {
                        incoming_message_cancel(
                            control,
                            info,
                            friend_public_key.clone(),
                            currency,
                            incoming_cancel,
                        )
                        .await
                    }
                }?;
            }

            // Possibly send RequestMoveToken back to friend in one of the following cases:
            if in_move_token_request.token_wanted {
                // Remote side wants to have the token back, so we have to send a MoveTokenRequest
                // to remote side, even if it is empty
                control
                    .access()
                    .send_commands
                    .move_token_allow_empty(friend_public_key.clone());
            } else {
                // We only send a MoveTokenRequest to remote side if we have something to send:
                control
                    .access()
                    .send_commands
                    .move_token(friend_public_key.clone());
            };
        }
        ReceiveMoveTokenOutput::ChainInconsistent(reset_terms) => {
            // TODO:
            // - Cancel relevant requests:
            //      - User pending requests
            //      - pending requests
            //
            // - Handle index mutations?
            //
            // - Send our reset terms
            //
            // - Notify to the outside?
            todo!();
        }
    }

    Ok(())
}

async fn incoming_inconsistency_error(
    control: &mut impl RouterControl,
    friend_public_key: PublicKey,
    reset_terms: ResetTerms,
) -> Result<(), RouterError> {
    todo!();
}

async fn incoming_relays_update(
    control: &mut impl RouterControl,
    friend_public_key: PublicKey,
    relays_update: RelaysUpdate,
) -> Result<(), RouterError> {
    todo!();
}

async fn incoming_relays_ack(
    control: &mut impl RouterControl,
    friend_public_key: PublicKey,
    generation: u128,
) -> Result<(), RouterError> {
    todo!();
}

pub async fn incoming_friend_message(
    control: &mut impl RouterControl,
    info: &RouterInfo,
    friend_public_key: PublicKey,
    friend_message: FriendMessage,
) -> Result<(), RouterError> {
    match friend_message {
        FriendMessage::MoveTokenRequest(move_token_request) => {
            incoming_move_token_request(control, info, friend_public_key, move_token_request).await
        }
        FriendMessage::InconsistencyError(reset_terms) => {
            incoming_inconsistency_error(control, friend_public_key, reset_terms).await
        }
        FriendMessage::RelaysUpdate(relays_update) => {
            incoming_relays_update(control, friend_public_key, relays_update).await
        }
        FriendMessage::RelaysAck(generation) => {
            incoming_relays_ack(control, friend_public_key, generation).await
        }
    }
}
