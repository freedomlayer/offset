use std::collections::{HashMap, HashSet};

use futures::StreamExt;

use derive_more::From;

use common::async_rpc::OpError;
use common::safe_arithmetic::{SafeSignedArithmetic, SafeUnsignedArithmetic};

use identity::IdentityClient;

use proto::app_server::messages::RelayAddressPort;
use proto::crypto::{NodePort, PublicKey, Signature};
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
use crate::router::utils::index_mutation::calc_recv_capacity;
use crate::token_channel::{handle_out_move_token, TcDbClient, TcStatus, TokenChannelError};

/*
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
*/

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
            return Ok(operations_vec);
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
            return Ok(operations_vec);
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
            return Ok(operations_vec);
        }
    }

    Ok(operations_vec)
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
        router_db_client
            .tc_db_client(friend_public_key.clone())
            .await?
            .ok_or(RouterError::InvalidDbState)?,
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

/// Like MoveToken, but without the calculated `info_hash` and `signature`
#[derive(Debug)]
struct PreMoveToken {
    pub currencies_operations: CurrenciesOperations,
    pub currencies_diff: Vec<Currency>,
}

/// Like MoveTokenRequest, but wrapping PreMoveToken instead of MoveToken.
#[derive(Debug)]
struct PreMoveTokenRequest {
    pub pre_move_token: PreMoveToken,
    pub token_wanted: bool,
}

/// Attempt to create an outgoing move token
/// Collect any information we need to send to remote friend:
///
/// - Currencies operations (requests, responses, cancels)
/// - Currencies diff (Added and removed currencies)
///
/// Without actually sending this information yet.
/// Return Ok(None) if we have nothing to send
async fn collect_outgoing_pre_move_token(
    router_db_client: &mut impl RouterDbClient,
    friend_public_key: PublicKey,
    max_operations_in_batch: usize,
) -> Result<Option<PreMoveTokenRequest>, RouterError> {
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
            Some(PreMoveTokenRequest {
                pre_move_token: PreMoveToken {
                    currencies_operations,
                    currencies_diff,
                },
                token_wanted: is_pending_currencies_operations(router_db_client, friend_public_key)
                    .await?,
            })
        },
    )
}

async fn send_pre_move_token(
    router_db_client: &mut impl RouterDbClient,
    identity_client: &mut IdentityClient,
    local_public_key: &PublicKey,
    friend_public_key: PublicKey,
    pre_move_token_request: PreMoveTokenRequest,
) -> Result<MoveTokenRequest, RouterError> {
    let move_token = handle_out_move_token(
        router_db_client
            .tc_db_client(friend_public_key.clone())
            .await?
            .ok_or(RouterError::InvalidDbState)?,
        identity_client,
        pre_move_token_request.pre_move_token.currencies_operations,
        pre_move_token_request.pre_move_token.currencies_diff,
        local_public_key,
        &friend_public_key,
    )
    .await?;

    Ok(MoveTokenRequest {
        move_token,
        token_wanted: pre_move_token_request.token_wanted,
    })
}

/// Collect all mentioned currencies from a PreMoveToken
fn get_mentioned_currencies(pre_move_token: &PreMoveToken) -> HashSet<Currency> {
    // Collect all mentioned currencies:
    let mut currencies = HashSet::new();
    for currency in &pre_move_token.currencies_diff {
        currencies.insert(currency.clone());
    }

    for (currency, _operation) in &pre_move_token.currencies_operations {
        currencies.insert(currency.clone());
    }

    currencies
}

async fn get_currencies_info(
    router_db_client: &mut impl RouterDbClient,
    friend_public_key: PublicKey,
    currencies: &[Currency],
) -> Result<HashMap<Currency, CurrencyInfo>, RouterError> {
    let mut currencies_info = HashMap::<Currency, CurrencyInfo>::new();
    for currency in currencies.iter() {
        let opt_currency_info = router_db_client
            .get_currency_info(friend_public_key.clone(), currency.clone())
            .await?;

        if let Some(currency_info) = opt_currency_info {
            currencies_info.insert(currency.clone(), currency_info);
        }
    }
    Ok(currencies_info)
}

/// Get receive capacities for a given list of currencies
fn get_recv_capacities(
    currencies_info: &HashMap<Currency, CurrencyInfo>,
    friend_public_key: PublicKey,
    currencies: &[Currency],
) -> Result<HashMap<Currency, u128>, RouterError> {
    let mut recv_capacities = HashMap::<Currency, u128>::new();
    for currency in currencies.iter() {
        let recv_capacity = if let Some(currency_info) = currencies_info.get(currency) {
            calc_recv_capacity(&currency_info)?
        } else {
            0u128
        };

        recv_capacities.insert(currency.clone(), recv_capacity);
    }
    Ok(recv_capacities)
}

fn diff_capacities(
    friend_public_key: PublicKey,
    capacities_before: &HashMap<Currency, u128>,
    capacities_after: &HashMap<Currency, u128>,
    currencies_info: &HashMap<Currency, CurrencyInfo>,
) -> Result<Vec<IndexMutation>, RouterError> {
    let mut index_mutations = Vec::new();
    for (currency, capacity_before) in capacities_before {
        let capacity_after = capacities_after
            .get(&currency)
            .ok_or(RouterError::InvalidState)?;

        // let currency_info = currencies_info.get(currency

        if capacity_before != capacity_after {
            if *capacity_after == 0 {
                index_mutations.push(IndexMutation::RemoveFriendCurrency(RemoveFriendCurrency {
                    public_key: friend_public_key.clone(),
                    currency: currency.clone(),
                }));
            } else {
                // We should have this currency's info if the capacity after is nonzero:
                let currency_info = currencies_info
                    .get(&currency)
                    .ok_or(RouterError::InvalidState)?;

                index_mutations.push(IndexMutation::UpdateFriendCurrency(UpdateFriendCurrency {
                    public_key: friend_public_key.clone(),
                    currency: currency.clone(),
                    recv_capacity: *capacity_after,
                    rate: currency_info.rate.clone(),
                }));
            }
        }
    }
    Ok(index_mutations)
}

pub async fn collect_outgoing_move_token(
    router_db_client: &mut impl RouterDbClient,
    identity_client: &mut IdentityClient,
    local_public_key: &PublicKey,
    friend_public_key: PublicKey,
    max_operations_in_batch: usize,
) -> Result<Option<(MoveTokenRequest, Vec<IndexMutation>)>, RouterError> {
    let opt_pre_move_token_request = collect_outgoing_pre_move_token(
        router_db_client,
        friend_public_key.clone(),
        max_operations_in_batch,
    )
    .await?;

    let pre_move_token_request = if let Some(pre_move_token_request) = opt_pre_move_token_request {
        pre_move_token_request
    } else {
        return Ok(None);
    };

    // Collect all mentioned currencies:
    let currencies = get_mentioned_currencies(&pre_move_token_request.pre_move_token);

    let currencies_info_before = get_currencies_info(
        router_db_client,
        friend_public_key.clone(),
        currencies.iter().cloned().collect::<Vec<_>>().as_slice(),
    )
    .await?;

    // Record recv capacity for all interesting currencies
    let recv_capacities_before = get_recv_capacities(
        &currencies_info_before,
        friend_public_key.clone(),
        currencies.iter().cloned().collect::<Vec<_>>().as_slice(),
    )?;

    // Send MoveToken:
    let move_token_request = send_pre_move_token(
        router_db_client,
        identity_client,
        local_public_key,
        friend_public_key.clone(),
        pre_move_token_request,
    )
    .await?;

    let currencies_info_after = get_currencies_info(
        router_db_client,
        friend_public_key.clone(),
        currencies.iter().cloned().collect::<Vec<_>>().as_slice(),
    )
    .await?;

    // Record recv capacity for all interesting currencies
    let recv_capacities_after = get_recv_capacities(
        &currencies_info_after,
        friend_public_key.clone(),
        currencies.iter().cloned().collect::<Vec<_>>().as_slice(),
    )?;

    // Compare recv capacities and create index mutations:
    let index_mutations = diff_capacities(
        friend_public_key,
        &recv_capacities_before,
        &recv_capacities_after,
        &currencies_info_after,
    )?;

    Ok(Some((move_token_request, index_mutations)))
}

/*
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
                router_db_client
                    .tc_db_client(friend_public_key.clone())
                    .await?
                    .ok_or(RouterError::InvalidDbState)?,
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
*/

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
