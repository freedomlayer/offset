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
    CancelSendFundsOp, Currency, FriendTcOp, MoveToken, MoveTokenRequest, RelaysUpdate,
    RequestSendFundsOp, ResetTerms, ResponseSendFundsOp,
};
use proto::index_server::messages::{IndexMutation, RemoveFriendCurrency, UpdateFriendCurrency};
use proto::net::messages::NetAddress;

use crypto::rand::{CryptoRandom, RandGen};

use crate::route::Route;
use crate::router::types::{
    BackwardsOp, CurrencyInfo, RouterControl, RouterDbClient, RouterError, RouterInfo,
    RouterOutput, RouterState, SentRelay,
};
use crate::router::utils::flush::flush_friend;
use crate::router::utils::move_token::is_pending_move_token;

use crate::mutual_credit::{McCancel, McRequest, McResponse};

use crate::token_channel::{
    handle_in_move_token, ReceiveMoveTokenOutput, TcDbClient, TcStatus, TokenChannelError,
};

pub async fn send_request(
    control: &mut impl RouterControl,
    info: &RouterInfo,
    currency: Currency,
    mut request: McRequest,
) -> Result<(), RouterError> {
    // Make sure that the provided route is valid:
    if !request.route.is_valid() {
        return Err(RouterError::InvalidRoute);
    }

    if control
        .access()
        .router_db_client
        .is_local_request_exists(request.request_id.clone())
        .await?
    {
        // We already have a local request with the same id. Return back a cancel.
        control.access().output.add_incoming_cancel(
            currency,
            McCancel {
                request_id: request.request_id,
            },
        );
        return Ok(());
    }

    // We cut the first two public keys from the route:
    // Pop first route public key:
    let route_local_public_key = request.route.remove(0);
    if &route_local_public_key != &info.local_public_key {
        return Err(RouterError::InvalidRoute);
    }
    // Pop second route public key:
    let friend_public_key = request.route.remove(0);

    let opt_tc_db_client = control
        .access()
        .router_db_client
        .tc_db_client(friend_public_key.clone())
        .await?;

    let tc_db_client = if let Some(tc_db_client) = opt_tc_db_client {
        tc_db_client
    } else {
        // Friend is not ready to accept a request.
        // We cancel the request:
        control.access().output.add_incoming_cancel(
            currency,
            McCancel {
                request_id: request.request_id.clone(),
            },
        );
        return Ok(());
    };

    // Gather information about friend's channel and liveness:
    // let tc_status = tc_db_client.get_tc_status().await?;
    // let is_online = router_state.liveness.is_online(&friend_public_key);

    // Check if friend is ready:
    if tc_db_client.get_tc_status().await?.is_consistent()
        && control
            .access()
            .ephemeral
            .liveness
            .is_online(&friend_public_key)
    {
        // Keep request_id:
        let request_id = request.request_id.clone();

        // Push request:
        control
            .access()
            .router_db_client
            .pending_user_requests_push_back(friend_public_key.clone(), currency, request)
            .await?;

        // Mark request as created locally, so that we remember to punt the corresponding
        // response/cancel message later.
        control
            .access()
            .router_db_client
            .add_local_request(request_id)
            .await?;

        control
            .access()
            .send_commands
            .move_token(friend_public_key.clone());
    } else {
        control.access().output.add_incoming_cancel(
            currency,
            McCancel {
                request_id: request.request_id,
            },
        );
    }

    Ok(())
}

pub async fn send_response(
    control: &mut impl RouterControl,
    info: &RouterInfo,
    response: McResponse,
) -> Result<(), RouterError> {
    let opt_request_origin = control
        .access()
        .router_db_client
        .get_remote_pending_request_origin(response.request_id.clone())
        .await?;

    // Attempt to find which node originally sent us this request,
    // so that we can forward him the response.
    // If we can not find the request's origin, we discard the response.
    if let Some(request_origin) = opt_request_origin {
        control
            .access()
            .router_db_client
            .pending_backwards_push_back(
                request_origin.friend_public_key.clone(),
                BackwardsOp::Response(request_origin.currency, response),
            )
            .await?;

        control
            .access()
            .send_commands
            .move_token(request_origin.friend_public_key);
    }

    Ok(())
}

pub async fn send_cancel(
    control: &mut impl RouterControl,
    cancel: McCancel,
) -> Result<(), RouterError> {
    let opt_request_origin = control
        .access()
        .router_db_client
        .get_remote_pending_request_origin(cancel.request_id.clone())
        .await?;

    // Attempt to find which node originally sent us this request,
    // so that we can forward him the response.
    // If we can not find the request's origin, we discard the response.
    if let Some(request_origin) = opt_request_origin {
        control
            .access()
            .router_db_client
            .pending_backwards_push_back(
                request_origin.friend_public_key.clone(),
                BackwardsOp::Cancel(request_origin.currency, cancel),
            )
            .await?;

        control
            .access()
            .send_commands
            .move_token(request_origin.friend_public_key);
    }

    Ok(())
}
