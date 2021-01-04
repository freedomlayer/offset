use std::collections::{HashMap, HashSet};

use futures::StreamExt;

use derive_more::From;

use common::async_rpc::OpError;
use common::safe_arithmetic::{SafeSignedArithmetic, SafeUnsignedArithmetic};

use database::transaction::Transaction;

use identity::IdentityClient;

use proto::app_server::messages::RelayAddressPort;
use proto::crypto::{NodePort, PublicKey};
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
use crate::router::utils::index_mutation::{
    create_index_mutations_from_move_token, create_update_index_mutation,
};
use crate::router::utils::move_token::{
    collect_outgoing_move_token, collect_outgoing_move_token_allow_empty, is_pending_move_token,
};
use crate::token_channel::{
    handle_in_move_token, ReceiveMoveTokenOutput, TcDbClient, TcStatus, TokenChannelError,
};

pub async fn send_request(
    router_db_client: &mut impl RouterDbClient,
    router_state: &RouterState,
    currency: Currency,
    mut request: RequestSendFundsOp,
    identity_client: &mut IdentityClient,
    local_public_key: &PublicKey,
    max_operations_in_batch: usize,
) -> Result<RouterOutput, RouterError> {
    // Make sure that the provided route is valid:
    if !request.route.is_valid() {
        return Err(RouterError::InvalidRoute);
    }

    let mut output = RouterOutput::new();

    // We cut the first two public keys from the route:
    // Pop first route public key:
    let route_local_public_key = request.route.remove(0);
    if &route_local_public_key != local_public_key {
        return Err(RouterError::InvalidRoute);
    }
    // Pop second route public key:
    let friend_public_key = request.route.remove(0);

    // Gather information about friend's channel and liveness:
    let tc_status = router_db_client
        .tc_db_client(friend_public_key.clone())
        .get_tc_status()
        .await?;
    let is_online = router_state.liveness.is_online(&friend_public_key);

    // If friend is ready (online + consistent):
    // - push the request
    //      - If token is incoming:
    //          - Send an outoging token
    //      - Else (token is outgoing):
    //          - Request token
    // Else (Friend is not ready):
    // - Send a cancel
    match (tc_status, is_online) {
        (TcStatus::ConsistentIn(_), true) => {
            // Push request:
            router_db_client
                .pending_user_requests_push_back(friend_public_key.clone(), currency, request)
                .await?;

            // Create an outgoing move token if we have something to send.
            let opt_move_token_request = collect_outgoing_move_token(
                router_db_client,
                identity_client,
                local_public_key,
                friend_public_key.clone(),
                max_operations_in_batch,
            )
            .await?;

            if let Some(move_token_request) = opt_move_token_request {
                // We have something to send to remote side
                // Deduce index mutations according to move token:
                let index_mutations = create_index_mutations_from_move_token(
                    router_db_client,
                    friend_public_key.clone(),
                    &move_token_request.move_token,
                )
                .await?;
                for index_mutation in index_mutations {
                    output.add_index_mutation(index_mutation);
                }

                // Send move token request to remote side:
                output.add_friend_message(
                    friend_public_key.clone(),
                    FriendMessage::MoveTokenRequest(move_token_request),
                );
            }
        }
        (TcStatus::ConsistentOut(move_token_out, _opt_move_token_hashed_in), true) => {
            // Push request:
            router_db_client
                .pending_user_requests_push_back(friend_public_key.clone(), currency, request)
                .await?;

            // Resend outgoing move token,
            // possibly asking for the token if we have something to send
            output.add_friend_message(
                friend_public_key.clone(),
                FriendMessage::MoveTokenRequest(MoveTokenRequest {
                    move_token: move_token_out,
                    token_wanted: is_pending_move_token(
                        router_db_client,
                        friend_public_key.clone(),
                    )
                    .await?,
                }),
            );
        }
        _ => {
            // Friend is not ready to accept a request.
            // We cancel the request:
            output.add_incoming_cancel(CancelSendFundsOp {
                request_id: request.request_id,
            });
        }
    }

    Ok(output)
}

/// Attempt to send as much as possible through a token channel to remote side
/// Assumes that the token channel is in consistent state (Incoming / Outgoing).
async fn flush_friend(
    router_db_client: &mut impl RouterDbClient,
    friend_public_key: PublicKey,
    identity_client: &mut IdentityClient,
    local_public_key: &PublicKey,
    max_operations_in_batch: usize,
    router_output: &mut RouterOutput,
) -> Result<(), RouterError> {
    match router_db_client
        .tc_db_client(friend_public_key.clone())
        .get_tc_status()
        .await?
    {
        TcStatus::ConsistentIn(_) => {
            // Create an outgoing move token if we have something to send.
            let opt_move_token_request = collect_outgoing_move_token(
                router_db_client,
                identity_client,
                local_public_key,
                friend_public_key.clone(),
                max_operations_in_batch,
            )
            .await?;

            if let Some(move_token_request) = opt_move_token_request {
                // We have something to send to remote side:

                // Update index mutations:
                let index_mutations = create_index_mutations_from_move_token(
                    router_db_client,
                    friend_public_key.clone(),
                    &move_token_request.move_token,
                )
                .await?;
                for index_mutation in index_mutations {
                    router_output.add_index_mutation(index_mutation);
                }
                router_output.add_friend_message(
                    friend_public_key.clone(),
                    FriendMessage::MoveTokenRequest(move_token_request),
                );
            }
        }
        TcStatus::ConsistentOut(move_token_out, _opt_move_token_hashed_in) => {
            // Resend outgoing move token,
            // possibly asking for the token if we have something to send
            router_output.add_friend_message(
                friend_public_key.clone(),
                FriendMessage::MoveTokenRequest(MoveTokenRequest {
                    move_token: move_token_out,
                    token_wanted: is_pending_move_token(
                        router_db_client,
                        friend_public_key.clone(),
                    )
                    .await?,
                }),
            );
        }
        // This state is not possible, because we did manage to locate our request with this
        // friend:
        TcStatus::Inconsistent(..) => return Err(RouterError::UnexpectedTcStatus),
    }

    Ok(())
}

pub async fn send_response(
    router_db_client: &mut impl RouterDbClient,
    response: ResponseSendFundsOp,
    identity_client: &mut IdentityClient,
    local_public_key: &PublicKey,
    max_operations_in_batch: usize,
) -> Result<RouterOutput, RouterError> {
    let mut output = RouterOutput::new();

    let opt_request_origin = router_db_client
        .get_remote_pending_request_origin(response.request_id.clone())
        .await?;

    // Attempt to find which node originally sent us this request,
    // so that we can forward him the response.
    // If we can not find the request's origin, we discard the response.
    if let Some(request_origin) = opt_request_origin {
        router_db_client
            .pending_backwards_push_back(
                request_origin.friend_public_key.clone(),
                request_origin.currency,
                BackwardsOp::Response(response),
            )
            .await?;

        flush_friend(
            router_db_client,
            request_origin.friend_public_key,
            identity_client,
            local_public_key,
            max_operations_in_batch,
            &mut output,
        )
        .await?;
    }

    Ok(output)
}

pub async fn send_cancel(
    router_db_client: &mut impl RouterDbClient,
    cancel: CancelSendFundsOp,
    identity_client: &mut IdentityClient,
    local_public_key: &PublicKey,
    max_operations_in_batch: usize,
) -> Result<RouterOutput, RouterError> {
    let mut output = RouterOutput::new();

    let opt_request_origin = router_db_client
        .get_remote_pending_request_origin(cancel.request_id.clone())
        .await?;

    // Attempt to find which node originally sent us this request,
    // so that we can forward him the response.
    // If we can not find the request's origin, we discard the response.
    if let Some(request_origin) = opt_request_origin {
        router_db_client
            .pending_backwards_push_back(
                request_origin.friend_public_key.clone(),
                request_origin.currency,
                BackwardsOp::Cancel(cancel),
            )
            .await?;

        flush_friend(
            router_db_client,
            request_origin.friend_public_key,
            identity_client,
            local_public_key,
            max_operations_in_batch,
            &mut output,
        )
        .await?;
    }

    Ok(output)
}
