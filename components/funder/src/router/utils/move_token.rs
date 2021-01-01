use std::collections::{HashMap, HashSet};

use futures::StreamExt;

use derive_more::From;

use common::async_rpc::OpError;
use common::safe_arithmetic::{SafeSignedArithmetic, SafeUnsignedArithmetic};

use identity::IdentityClient;

use proto::app_server::messages::RelayAddressPort;
use proto::crypto::{NodePort, PublicKey};
use proto::funder::messages::{
    CancelSendFundsOp, CurrenciesOperations, Currency, CurrencyOperations, FriendMessage,
    FriendTcOp, MoveToken, MoveTokenRequest, RelaysUpdate, RequestSendFundsOp, ResponseSendFundsOp,
};
use proto::index_server::messages::{IndexMutation, RemoveFriendCurrency, UpdateFriendCurrency};
use proto::net::messages::NetAddress;

use crypto::rand::{CryptoRandom, RandGen};

use crate::route::Route;
use crate::router::types::{
    BackwardsOp, CurrencyInfo, RouterDbClient, RouterError, RouterOutput, RouterState, SentRelay,
};
use crate::token_channel::{handle_out_move_token, TcDbClient, TcStatus, TokenChannelError};

fn operations_vec_to_currencies_operations(
    operations_vec: Vec<(Currency, FriendTcOp)>,
) -> CurrenciesOperations {
    let mut currencies_operations = HashMap::<Currency, Vec<FriendTcOp>>::new();
    for (currency, tc_op) in operations_vec {
        let entry = currencies_operations.entry(currency).or_insert(Vec::new());
        (*entry).push(tc_op);
    }
    currencies_operations
}

async fn collect_currencies_operations(
    router_db_client: &mut impl RouterDbClient,
    friend_public_key: PublicKey,
    max_operations_in_batch: usize,
) -> Result<CurrenciesOperations, RouterError> {
    let mut operations_vec = Vec::<(Currency, FriendTcOp)>::new();

    // Collect any pending responses and cancels:
    while let Some((currency, backwards_op)) = router_db_client
        .pending_backwards_pop_front(friend_public_key.clone())
        .await?
    {
        let friend_tc_op = match backwards_op {
            BackwardsOp::Response(response_op) => FriendTcOp::ResponseSendFunds(response_op),
            BackwardsOp::Cancel(cancel_op) => FriendTcOp::CancelSendFunds(cancel_op),
        };
        operations_vec.push((currency, friend_tc_op));

        // Make sure we do not exceed maximum amount of operations:
        if operations_vec.len() >= max_operations_in_batch {
            return Ok(operations_vec_to_currencies_operations(operations_vec));
        }
    }

    // Collect any pending user requests:
    while let Some((currency, request_op)) = router_db_client
        .pending_user_requests_pop_front(friend_public_key.clone())
        .await?
    {
        let friend_tc_op = FriendTcOp::RequestSendFunds(request_op);
        operations_vec.push((currency, friend_tc_op));

        // Make sure we do not exceed maximum amount of operations:
        if operations_vec.len() >= max_operations_in_batch {
            return Ok(operations_vec_to_currencies_operations(operations_vec));
        }
    }

    // Collect any pending requests:
    while let Some((currency, request_op)) = router_db_client
        .pending_requests_pop_front(friend_public_key.clone())
        .await?
    {
        let friend_tc_op = FriendTcOp::RequestSendFunds(request_op);
        operations_vec.push((currency, friend_tc_op));

        // Make sure we do not exceed maximum amount of operations:
        if operations_vec.len() >= max_operations_in_batch {
            return Ok(operations_vec_to_currencies_operations(operations_vec));
        }
    }

    Ok(operations_vec_to_currencies_operations(operations_vec))
}

/// Do we have more pending currencies operations?
async fn is_pending_currencies_operations(
    router_db_client: &mut impl RouterDbClient,
    friend_public_key: PublicKey,
) -> Result<bool, RouterError> {
    Ok(!router_db_client
        .pending_backwards_is_empty(friend_public_key.clone())
        .await?
        || !router_db_client
            .pending_user_requests_is_empty(friend_public_key.clone())
            .await?
        || !router_db_client
            .pending_requests_is_empty(friend_public_key.clone())
            .await?)
}

/// Attempt to create an outgoing move token
/// May create an empty move token.
pub async fn collect_outgoing_move_token_allow_empty(
    router_db_client: &mut impl RouterDbClient,
    identity_client: &mut IdentityClient,
    local_public_key: &PublicKey,
    friend_public_key: PublicKey,
    max_operations_in_batch: usize,
) -> Result<MoveTokenRequest, RouterError> {
    let currencies_operations = collect_currencies_operations(
        router_db_client,
        friend_public_key.clone(),
        max_operations_in_batch,
    )
    .await?;

    let mut currencies_diff = router_db_client
        .currencies_diff(friend_public_key.clone())
        .await?;

    // Create move token and update internal state:
    let move_token = handle_out_move_token(
        router_db_client.tc_db_client(friend_public_key.clone()),
        identity_client,
        currencies_operations,
        currencies_diff,
        local_public_key,
        &friend_public_key,
    )
    .await?;

    Ok(MoveTokenRequest {
        move_token,
        token_wanted: is_pending_currencies_operations(router_db_client, friend_public_key).await?,
    })
}

/// Attempt to create an outgoing move token
/// Return Ok(None) if we have nothing to send
pub async fn collect_outgoing_move_token(
    router_db_client: &mut impl RouterDbClient,
    identity_client: &mut IdentityClient,
    local_public_key: &PublicKey,
    friend_public_key: PublicKey,
    max_operations_in_batch: usize,
) -> Result<Option<MoveTokenRequest>, RouterError> {
    let currencies_operations = collect_currencies_operations(
        router_db_client,
        friend_public_key.clone(),
        max_operations_in_batch,
    )
    .await?;

    let mut currencies_diff = router_db_client
        .currencies_diff(friend_public_key.clone())
        .await?;

    Ok(
        if currencies_operations.is_empty() && currencies_diff.is_empty() {
            // There is nothing interesting to send to remote side
            None
        } else {
            // We have something to send to remote side
            let move_token = handle_out_move_token(
                router_db_client.tc_db_client(friend_public_key.clone()),
                identity_client,
                currencies_operations,
                currencies_diff,
                local_public_key,
                &friend_public_key,
            )
            .await?;
            Some(MoveTokenRequest {
                move_token,
                token_wanted: is_pending_currencies_operations(router_db_client, friend_public_key)
                    .await?,
            })
        },
    )
}

/// Check if we have anything to send to a remove friend on a move token message,
/// without performing any data mutations
pub async fn is_pending_move_token(
    router_db_client: &mut impl RouterDbClient,
    friend_public_key: PublicKey,
) -> Result<bool, RouterError> {
    Ok(
        is_pending_currencies_operations(router_db_client, friend_public_key.clone()).await?
            || !router_db_client
                .currencies_diff(friend_public_key.clone())
                .await?
                .is_empty(),
    )
}
