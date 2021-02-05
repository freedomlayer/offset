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
    CancelSendFundsOp, CurrenciesOperations, Currency, CurrencyOperations, FriendMessage,
    FriendTcOp, MoveToken, MoveTokenRequest, RelaysUpdate, RequestSendFundsOp, ResetTerms,
    ResponseSendFundsOp,
};
use proto::index_server::messages::{IndexMutation, RemoveFriendCurrency, UpdateFriendCurrency};
use proto::net::messages::NetAddress;

use crypto::rand::{CryptoRandom, RandGen};

use crate::route::Route;
use crate::router::types::{
    BackwardsOp, CurrencyInfo, RouterDbClient, RouterError, RouterOutput, RouterState, SentRelay,
};

use crate::mutual_credit::incoming::{
    IncomingCancelSendFundsOp, IncomingMessage, IncomingResponseSendFundsOp,
};
use crate::router::utils::flush::flush_friend;
use crate::router::utils::index_mutation::create_update_index_mutation;
use crate::router::utils::move_token::{
    handle_in_move_token_index_mutations, handle_out_move_token_index_mutations_allow_empty,
    handle_out_move_token_index_mutations_disallow_empty, is_pending_move_token,
};
use crate::token_channel::{ReceiveMoveTokenOutput, TcDbClient, TcStatus, TokenChannelError};

/// Check if credit line between this node and a friend is ready
/// Works for sending requests in both directions
async fn is_credit_line_ready(
    router_db_client: &mut impl RouterDbClient,
    router_state: &RouterState,
    friend_public_key: PublicKey,
    currency: Currency,
    request_id: Uid,
) -> Result<bool, RouterError> {
    // Friend must be online:
    if !router_state.liveness.is_online(&friend_public_key) {
        return Ok(false);
    }

    // Currency must exist:
    let currency_info = if let Some(currency_info) = router_db_client
        .get_currency_info(friend_public_key, currency)
        .await?
    {
        currency_info
    } else {
        return Ok(false);
    };

    if let Some(currency_info_local) = currency_info.opt_local {
        if let Some(mc_balance) = currency_info_local.opt_remote {
            // A mutual credit line is open for this currency
        } else {
            return Ok(false);
        }
    } else {
        return Ok(false);
    }

    Ok(currency_info.is_open || router_db_client.is_request_local_origin(request_id).await?)
}

async fn incoming_message_request<RC>(
    mut router_db_client: &mut RC,
    router_state: &mut RouterState,
    friend_public_key: PublicKey,
    identity_client: &mut IdentityClient,
    local_public_key: &PublicKey,
    max_operations_in_batch: usize,
    router_output: &mut RouterOutput,
    currency: Currency,
    mut request_send_funds: RequestSendFundsOp,
) -> Result<(), RouterError>
where
    RC: RouterDbClient,
{
    // Make sure that we are ready to receive this request operation
    // We might not be ready in the case of currency being closed.
    if !is_credit_line_ready(
        router_db_client,
        &*router_state,
        friend_public_key.clone(),
        currency.clone(),
        request_send_funds.request_id.clone(),
    )
    .await?
    {
        // Return a cancel message and flush origin friend
        router_db_client
            .pending_backwards_push_back(
                friend_public_key.clone(),
                currency,
                BackwardsOp::Cancel(CancelSendFundsOp {
                    request_id: request_send_funds.request_id.clone(),
                }),
            )
            .await?;

        flush_friend(
            router_db_client,
            friend_public_key,
            identity_client,
            local_public_key,
            max_operations_in_batch,
            router_output,
        )
        .await?;

        return Ok(());
    }

    // If directed to us, punt
    if request_send_funds.route.is_empty() {
        // Directed to us. Punt:
        router_output.add_incoming_request(request_send_funds);
        return Ok(());
    }

    // - If directed to another friend:
    //      - If friend exists and ready, forward to next hop
    //      - Otherwise, queue cancel
    // Route is not empty. We need to forward the request to a friend
    let next_public_key = request_send_funds.route.remove(0);

    // Check if next public key corresponds to a friend that is ready
    let should_forward = is_credit_line_ready(
        router_db_client,
        &*router_state,
        next_public_key.clone(),
        currency.clone(),
        request_send_funds.request_id.clone(),
    )
    .await?
        && !router_db_client
            .is_local_request_exists(request_send_funds.request_id.clone())
            .await?;

    // Attempt to collect fees according to rate. If credits in fees jar are insufficient, do not
    // forward, and cancel the request.
    todo!();

    if should_forward {
        // Queue request to friend and flush destination friend
        router_db_client
            .pending_requests_push_back(next_public_key.clone(), currency, request_send_funds)
            .await?;
        flush_friend(
            router_db_client,
            next_public_key,
            identity_client,
            local_public_key,
            max_operations_in_batch,
            router_output,
        )
        .await?;
    } else {
        // Return a cancel message and flush origin friend
        router_db_client
            .pending_backwards_push_back(
                friend_public_key.clone(),
                currency,
                BackwardsOp::Cancel(CancelSendFundsOp {
                    request_id: request_send_funds.request_id.clone(),
                }),
            )
            .await?;

        flush_friend(
            router_db_client,
            friend_public_key,
            identity_client,
            local_public_key,
            max_operations_in_batch,
            router_output,
        )
        .await?;
    }

    Ok(())
}

/// An incoming request was received, but due to insufficient trust we can not
/// forward this request. We return a cancel message to the sender friend
async fn incoming_message_request_cancel<RC>(
    mut router_db_client: &mut RC,
    friend_public_key: PublicKey,
    identity_client: &mut IdentityClient,
    router_output: &mut RouterOutput,
    currency: Currency,
    request_send_funds: RequestSendFundsOp,
) -> Result<(), RouterError>
where
    RC: RouterDbClient,
{
    if router_db_client
        .tc_db_client(friend_public_key.clone())
        .await?
        .ok_or(RouterError::InvalidState)?
        .get_tc_status()
        .await?
        .is_consistent()
    {
        // TODO: Something is strange here. Are we actually queueing a cancel message or something
        // else?

        // Queue cancel to friend_public_key (Request origin)
        router_db_client
            .pending_requests_push_back(friend_public_key.clone(), currency, request_send_funds)
            .await?;
        todo!();
    } else {
        // We have just received a cancel message from this friend.
        // We expect that this friend is consistent
        return Err(RouterError::InvalidState);
    }
    Ok(())
}

async fn incoming_message_response<RC>(
    mut router_db_client: &mut RC,
    router_state: &mut RouterState,
    friend_public_key: PublicKey,
    identity_client: &mut IdentityClient,
    local_public_key: &PublicKey,
    max_operations_in_batch: usize,
    router_output: &mut RouterOutput,
    currency: Currency,
    incoming_response: IncomingResponseSendFundsOp,
) -> Result<(), RouterError>
where
    RC: RouterDbClient,
{
    // Gather information about request's origin:
    let opt_request_origin = router_db_client
        .get_remote_pending_request_origin(incoming_response.incoming_response.request_id.clone())
        .await?;
    let is_local_origin = router_db_client
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
            router_db_client
                .remove_local_request(incoming_response.incoming_response.request_id.clone())
                .await?;

            // We punt the response
            router_output.add_incoming_response(incoming_response.incoming_response);
        }
        (false, Some(request_origin)) => {
            // Request has originated from a friend.
            // We assume that friend must be consistent, otherwise it would have not been found as
            // an origin.

            // Push response:
            router_db_client
                .pending_backwards_push_back(
                    friend_public_key.clone(),
                    currency,
                    BackwardsOp::Response(incoming_response.incoming_response),
                )
                .await?;

            // Attempt to send a move token if possible
            flush_friend(
                router_db_client,
                friend_public_key.clone(),
                identity_client,
                local_public_key,
                max_operations_in_batch,
                router_output,
            )
            .await?;
        }
        (true, Some(request_origin)) => {
            // This means the request has both originated locally, and from a friend.
            // Note that this only happen during a cycle request, but a cycle always begins and
            // ends at the local node.
            // Therefore, this should never happen when processing a friend incoming response.
            unreachable!();
        }
    }
    Ok(())
}

// TODO: This function seems too similar to`incoming_message_response`
async fn incoming_message_cancel<RC>(
    mut router_db_client: &mut RC,
    router_state: &mut RouterState,
    friend_public_key: PublicKey,
    identity_client: &mut IdentityClient,
    local_public_key: &PublicKey,
    max_operations_in_batch: usize,
    router_output: &mut RouterOutput,
    currency: Currency,
    incoming_cancel: IncomingCancelSendFundsOp,
) -> Result<(), RouterError>
where
    RC: RouterDbClient,
{
    // Gather information about request's origin:
    let opt_request_origin = router_db_client
        .get_remote_pending_request_origin(incoming_cancel.incoming_cancel.request_id.clone())
        .await?;
    let is_local_origin = router_db_client
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
            router_db_client
                .remove_local_request(incoming_cancel.incoming_cancel.request_id.clone())
                .await?;

            // We punt the cancel
            router_output.add_incoming_cancel(incoming_cancel.incoming_cancel);
        }
        (false, Some(request_origin)) => {
            // Request has originated from a friend.
            // We assume that friend must be consistent, otherwise it would have not been found as
            // an origin.

            // Push cancel:
            router_db_client
                .pending_backwards_push_back(
                    friend_public_key.clone(),
                    currency,
                    BackwardsOp::Cancel(incoming_cancel.incoming_cancel),
                )
                .await?;

            // Attempt to send a move token if possible
            flush_friend(
                router_db_client,
                friend_public_key.clone(),
                identity_client,
                local_public_key,
                max_operations_in_batch,
                router_output,
            )
            .await?;
        }
        (true, Some(request_origin)) => {
            // This means the request has both originated locally, and from a friend.
            // Note that this only happen during a cycle request, but a cycle always begins and
            // ends at the local node.
            // Therefore, this should never happen when processing a friend incoming cancel.
            unreachable!();
        }
    }
    Ok(())
}

async fn incoming_move_token_request<RC>(
    mut router_db_client: &mut RC,
    router_state: &mut RouterState,
    friend_public_key: PublicKey,
    identity_client: &mut IdentityClient,
    local_public_key: &PublicKey,
    max_operations_in_batch: usize,
    in_move_token_request: MoveTokenRequest,
) -> Result<RouterOutput, RouterError>
where
    RC: RouterDbClient,
    // TODO: Maybe not necessary:
    RC::TcDbClient: Transaction + Send,
    <RC::TcDbClient as TcDbClient>::McDbClient: Send,
{
    let mut output = RouterOutput::new();

    let (receive_move_token_output, index_mutations) = handle_in_move_token_index_mutations(
        router_db_client,
        identity_client,
        in_move_token_request.move_token,
        local_public_key,
        friend_public_key.clone(),
    )
    .await?;

    // Add mutations:
    for index_mutation in index_mutations {
        output.add_index_mutation(index_mutation);
    }

    match receive_move_token_output {
        ReceiveMoveTokenOutput::Duplicate => {
            // We should have nothing else to send at this point, otherwise we would have already
            // sent it.
            assert!(!is_pending_move_token(router_db_client, friend_public_key.clone()).await?);

            // Possibly send token to remote side (According to token_wanted)
            if in_move_token_request.token_wanted {
                let (out_move_token_request, index_mutations) =
                    handle_out_move_token_index_mutations_allow_empty(
                        router_db_client,
                        identity_client,
                        local_public_key,
                        friend_public_key.clone(),
                        max_operations_in_batch,
                    )
                    .await?;

                // We just got a message from remote side. Remote side should be online:
                assert!(router_state.liveness.is_online(&friend_public_key));

                // Add index mutations:
                for index_mutation in index_mutations {
                    output.add_index_mutation(index_mutation);
                }
                output.add_friend_message(
                    friend_public_key.clone(),
                    FriendMessage::MoveTokenRequest(out_move_token_request),
                );
            }
        }
        ReceiveMoveTokenOutput::RetransmitOutgoing(move_token) => {
            // Retransmit outgoing move token message to friend:
            let move_token_request = MoveTokenRequest {
                move_token,
                token_wanted: is_pending_move_token(router_db_client, friend_public_key.clone())
                    .await?,
            };

            if router_state.liveness.is_online(&friend_public_key) {
                output.add_friend_message(
                    friend_public_key.clone(),
                    FriendMessage::MoveTokenRequest(move_token_request),
                );
            }
        }
        ReceiveMoveTokenOutput::Received(move_token_received) => {
            for (currency, incoming_message) in move_token_received.incoming_messages {
                match incoming_message {
                    IncomingMessage::Request(request_send_funds) => {
                        incoming_message_request(
                            router_db_client,
                            router_state,
                            friend_public_key.clone(),
                            identity_client,
                            local_public_key,
                            max_operations_in_batch,
                            &mut output,
                            currency,
                            request_send_funds,
                        )
                        .await
                    }
                    IncomingMessage::RequestCancel(request_send_funds) => {
                        incoming_message_request_cancel(
                            router_db_client,
                            friend_public_key.clone(),
                            identity_client,
                            &mut output,
                            currency,
                            request_send_funds,
                        )
                        .await
                    }
                    IncomingMessage::Response(incoming_response) => {
                        incoming_message_response(
                            router_db_client,
                            router_state,
                            friend_public_key.clone(),
                            identity_client,
                            local_public_key,
                            max_operations_in_batch,
                            &mut output,
                            currency,
                            incoming_response,
                        )
                        .await
                    }
                    IncomingMessage::Cancel(incoming_cancel) => {
                        incoming_message_cancel(
                            router_db_client,
                            router_state,
                            friend_public_key.clone(),
                            identity_client,
                            local_public_key,
                            max_operations_in_batch,
                            &mut output,
                            currency,
                            incoming_cancel,
                        )
                        .await
                    }
                }?;
            }

            // Possibly send RequestMoveToken back to friend in one of the following cases:
            let opt_res = if in_move_token_request.token_wanted {
                // Remote side wants to have the token back, so we have to send a MoveTokenRequest
                // to remote side, even if it is empty
                Some(
                    handle_out_move_token_index_mutations_allow_empty(
                        router_db_client,
                        identity_client,
                        local_public_key,
                        friend_public_key.clone(),
                        max_operations_in_batch,
                    )
                    .await?,
                )
            } else {
                // We only send a MoveTokenRequest to remote side if we have something to send:
                handle_out_move_token_index_mutations_disallow_empty(
                    router_db_client,
                    identity_client,
                    local_public_key,
                    friend_public_key.clone(),
                    max_operations_in_batch,
                )
                .await?
            };

            if let Some((out_move_token_request, index_mutations)) = opt_res {
                // We just got a message from remote side. Remote side should be online:
                assert!(router_state.liveness.is_online(&friend_public_key));

                // Add index mutations:
                for index_mutation in index_mutations {
                    output.add_index_mutation(index_mutation);
                }
                output.add_friend_message(
                    friend_public_key.clone(),
                    FriendMessage::MoveTokenRequest(out_move_token_request),
                );
            }
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

    Ok(output)
}

async fn incoming_inconsistency_error(
    router_db_client: &mut impl RouterDbClient,
    friend_public_key: PublicKey,
    reset_terms: ResetTerms,
) -> Result<RouterOutput, RouterError> {
    todo!();
}

async fn incoming_relays_update(
    router_db_client: &mut impl RouterDbClient,
    friend_public_key: PublicKey,
    relays_update: RelaysUpdate,
) -> Result<RouterOutput, RouterError> {
    todo!();
}

async fn incoming_relays_ack(
    router_db_client: &mut impl RouterDbClient,
    friend_public_key: PublicKey,
    generation: u128,
) -> Result<RouterOutput, RouterError> {
    todo!();
}

pub async fn incoming_friend_message<RC>(
    router_db_client: &mut RC,
    router_state: &mut RouterState,
    friend_public_key: PublicKey,
    identity_client: &mut IdentityClient,
    local_public_key: &PublicKey,
    max_operations_in_batch: usize,
    friend_message: FriendMessage,
) -> Result<RouterOutput, RouterError>
where
    RC: RouterDbClient,
    RC::TcDbClient: Transaction + Send,
    <RC::TcDbClient as TcDbClient>::McDbClient: Send,
{
    match friend_message {
        FriendMessage::MoveTokenRequest(move_token_request) => {
            incoming_move_token_request(
                router_db_client,
                router_state,
                friend_public_key,
                identity_client,
                local_public_key,
                max_operations_in_batch,
                move_token_request,
            )
            .await
        }
        FriendMessage::InconsistencyError(reset_terms) => {
            incoming_inconsistency_error(router_db_client, friend_public_key, reset_terms).await
        }
        FriendMessage::RelaysUpdate(relays_update) => {
            incoming_relays_update(router_db_client, friend_public_key, relays_update).await
        }
        FriendMessage::RelaysAck(generation) => {
            incoming_relays_ack(router_db_client, friend_public_key, generation).await
        }
    }
}
