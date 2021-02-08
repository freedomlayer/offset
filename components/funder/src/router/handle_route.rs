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
    CancelSendFundsOp, Currency, FriendMessage, FriendTcOp, MoveToken, MoveTokenRequest,
    RelaysUpdate, RequestSendFundsOp, ResetTerms, ResponseSendFundsOp,
};
use proto::index_server::messages::{IndexMutation, RemoveFriendCurrency, UpdateFriendCurrency};
use proto::net::messages::NetAddress;

use crypto::rand::{CryptoRandom, RandGen};

use crate::route::Route;
use crate::router::types::{
    BackwardsOp, CurrencyInfo, RouterDbClient, RouterError, RouterOutput, RouterState, SentRelay,
};
use crate::router::utils::flush::flush_friend;
use crate::router::utils::move_token::is_pending_move_token;

use crate::mutual_credit::{McCancel, McRequest, McResponse};

use crate::token_channel::{
    handle_in_move_token, ReceiveMoveTokenOutput, TcDbClient, TcStatus, TokenChannelError,
};

pub async fn send_request(
    router_db_client: &mut impl RouterDbClient,
    router_state: &RouterState,
    currency: Currency,
    mut request: McRequest,
    identity_client: &mut IdentityClient,
    local_public_key: &PublicKey,
    max_operations_in_batch: usize,
) -> Result<RouterOutput, RouterError> {
    // Make sure that the provided route is valid:
    if !request.route.is_valid() {
        return Err(RouterError::InvalidRoute);
    }

    let mut output = RouterOutput::new();

    if router_db_client
        .is_local_request_exists(request.request_id.clone())
        .await?
    {
        // We already have a local request with the same id. Return back a cancel.
        output.add_incoming_cancel(
            currency,
            McCancel {
                request_id: request.request_id,
            },
        );
        return Ok(output);
    }

    // We cut the first two public keys from the route:
    // Pop first route public key:
    let route_local_public_key = request.route.remove(0);
    if &route_local_public_key != local_public_key {
        return Err(RouterError::InvalidRoute);
    }
    // Pop second route public key:
    let friend_public_key = request.route.remove(0);

    let opt_tc_db_client = router_db_client
        .tc_db_client(friend_public_key.clone())
        .await?;

    let tc_db_client = if let Some(tc_db_client) = opt_tc_db_client {
        tc_db_client
    } else {
        // Friend is not ready to accept a request.
        // We cancel the request:
        output.add_incoming_cancel(
            currency,
            McCancel {
                request_id: request.request_id.clone(),
            },
        );
        return Ok(output);
    };

    // Gather information about friend's channel and liveness:
    // let tc_status = tc_db_client.get_tc_status().await?;
    // let is_online = router_state.liveness.is_online(&friend_public_key);

    // Check if friend is ready:
    if tc_db_client.get_tc_status().await?.is_consistent()
        && router_state.liveness.is_online(&friend_public_key)
    {
        // Keep request_id:
        let request_id = request.request_id.clone();

        // Push request:
        router_db_client
            .pending_user_requests_push_back(friend_public_key.clone(), currency, request)
            .await?;

        // Mark request as created locally, so that we remember to punt the corresponding
        // response/cancel message later.
        router_db_client.add_local_request(request_id).await?;

        // TODO: Change flush_friend design
        todo!();
        flush_friend(
            router_db_client,
            friend_public_key.clone(),
            identity_client,
            local_public_key,
            max_operations_in_batch,
            &mut output,
        )
        .await?;
    } else {
        output.add_incoming_cancel(
            currency,
            McCancel {
                request_id: request.request_id,
            },
        );
    }

    Ok(output)
}

pub async fn send_response(
    router_db_client: &mut impl RouterDbClient,
    response: McResponse,
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
                BackwardsOp::Response(request_origin.currency, response),
            )
            .await?;

        // TODO: Change flush_friend design
        todo!();
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
    cancel: McCancel,
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
                BackwardsOp::Cancel(request_origin.currency, cancel),
            )
            .await?;

        // TODO: Change flush_friend design
        todo!();
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
