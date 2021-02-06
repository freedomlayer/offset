use std::collections::{HashMap, HashSet};

use futures::StreamExt;

use derive_more::From;

use common::async_rpc::OpError;
use common::safe_arithmetic::{SafeSignedArithmetic, SafeUnsignedArithmetic};

use identity::IdentityClient;

use proto::app_server::messages::RelayAddressPort;
use proto::crypto::{NodePort, PublicKey, Signature};
use proto::funder::messages::{
    CancelSendFundsOp, Currency, FriendMessage, FriendTcOp, MoveToken, MoveTokenRequest,
    RelaysUpdate, RequestSendFundsOp, ResponseSendFundsOp,
};
use proto::index_server::messages::{IndexMutation, RemoveFriendCurrency, UpdateFriendCurrency};
use proto::net::messages::NetAddress;

use crypto::rand::{CryptoRandom, RandGen};

use database::transaction::Transaction;

use crate::mutual_credit::{McCancel, McRequest, McResponse};
use crate::route::Route;
use crate::router::types::{
    BackwardsOp, CurrencyInfo, FriendBalance, FriendBalanceDiff, RouterDbClient, RouterError,
    RouterOutput, RouterState, SentRelay,
};
use crate::router::utils::index_mutation::calc_capacities;
use crate::token_channel::{
    handle_in_move_token, OutMoveToken, ReceiveMoveTokenOutput, TcDbClient, TcOp, TcStatus,
    TokenChannelError,
};

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

async fn queue_backwards_op(
    router_db_client: &mut impl RouterDbClient,
    out_move_token: &mut OutMoveToken,
    local_public_key: &PublicKey,
    friend_public_key: PublicKey,
    backwards_op: BackwardsOp,
) -> Result<(), RouterError> {
    let tc_db_client = if let Some(tc_db_client) = router_db_client
        .tc_db_client(friend_public_key.clone())
        .await?
    {
        tc_db_client
    } else {
        return Err(RouterError::InvalidState);
    };
    let friend_tc_op = match backwards_op {
        BackwardsOp::Response(currency, mc_response) => {
            out_move_token
                .queue_response(tc_db_client, currency, mc_response, local_public_key)
                .await?;
        }
        BackwardsOp::Cancel(currency, mc_cancel) => {
            out_move_token
                .queue_cancel(tc_db_client, currency, mc_cancel)
                .await?;
        }
    };
    Ok(())
}

/// Queue a request to an `out_move_token`.
/// Handles failure by returning a cancel message to the relevant origin.
async fn queue_request(
    router_db_client: &mut impl RouterDbClient,
    out_move_token: &mut OutMoveToken,
    local_public_key: &PublicKey,
    friend_public_key: PublicKey,
    currency: Currency,
    mc_request: McRequest,
    router_output: &mut RouterOutput,
) -> Result<(), RouterError> {
    let tc_db_client = if let Some(tc_db_client) = router_db_client
        .tc_db_client(friend_public_key.clone())
        .await?
    {
        tc_db_client
    } else {
        return Err(RouterError::InvalidState);
    };

    let res = out_move_token
        .queue_request(tc_db_client, currency.clone(), mc_request.clone())
        .await?;

    match res {
        Ok(mc_balance) => {
            let currency_info = router_db_client
                .get_currency_info(friend_public_key.clone(), currency.clone())
                .await?
                // If currency does not exist, we should have already cancelled this request.
                .ok_or(RouterError::InvalidState)?;

            // If currency is marked for removal, and all balances are zero, remove currency:
            if currency_info.is_remove
                && mc_balance.local_pending_debt == 0
                && mc_balance.remote_pending_debt == 0
                && mc_balance.balance == 0
            {
                // Remove currency:
                router_db_client
                    .remove_currency(friend_public_key.clone(), currency.clone())
                    .await?;

                // Add event of currency removal (Friend event)
                let balances_diff = {
                    let mut balances_diff = HashMap::new();
                    let friend_balance_diff = FriendBalanceDiff {
                        old_balance: FriendBalance {
                            balance: 0,
                            in_fees: mc_balance.in_fees,
                            out_fees: mc_balance.out_fees,
                        },
                        new_balance: FriendBalance {
                            balance: 0,
                            // TODO: Are we calculating in_fees and out_fees correctly here?
                            in_fees: 0.into(),
                            out_fees: 0.into(),
                        },
                    };
                    balances_diff.insert(currency.clone(), friend_balance_diff);
                    balances_diff
                };
                router_db_client
                    .add_friend_event(friend_public_key.clone(), balances_diff)
                    .await?;
            }
        }
        Err(mc_cancel) => {
            // We need to send a cancel message to the origin
            if router_db_client
                .is_request_local_origin(mc_request.request_id.clone())
                .await?
            {
                // Request is of local origin
                router_output.add_incoming_cancel(McCancel {
                    request_id: mc_request.request_id.clone(),
                });
            } else {
                if let Some(request_origin) = router_db_client
                    .get_remote_pending_request_origin(mc_request.request_id.clone())
                    .await?
                {
                    // Request is of remote origin
                    router_db_client.pending_backwards_push_back(
                        request_origin.friend_public_key.clone(),
                        request_origin.currency.clone(),
                        BackwardsOp::Cancel(
                            request_origin.currency.clone(),
                            McCancel {
                                request_id: mc_request.request_id.clone(),
                            },
                        ),
                    );
                } else {
                    // Request is orphan, nothing to do here
                }
            }
        }
    }
    Ok(())
}

async fn collect_currencies_operations(
    router_db_client: &mut impl RouterDbClient,
    local_public_key: &PublicKey,
    friend_public_key: PublicKey,
    max_operations_in_batch: usize,
    router_output: &mut RouterOutput,
) -> Result<OutMoveToken, RouterError> {
    // Create a structure that aggregates operations to be sent in a single MoveToken message:
    let mut out_move_token = OutMoveToken::new();

    // Collect any pending responses and cancels:
    while let Some((currency, backwards_op)) = router_db_client
        .pending_backwards_pop_front(friend_public_key.clone())
        .await?
    {
        queue_backwards_op(
            router_db_client,
            &mut out_move_token,
            local_public_key,
            friend_public_key.clone(),
            backwards_op,
        )
        .await?;

        // Make sure we do not exceed maximum amount of operations:
        if out_move_token.len() >= max_operations_in_batch {
            return Ok(out_move_token);
        }
    }

    // Collect any pending user requests:
    while let Some((currency, mc_request)) = router_db_client
        .pending_user_requests_pop_front(friend_public_key.clone())
        .await?
    {
        queue_request(
            router_db_client,
            &mut out_move_token,
            local_public_key,
            friend_public_key.clone(),
            currency,
            mc_request,
            router_output,
        )
        .await?;

        // Make sure we do not exceed maximum amount of operations:
        if out_move_token.len() >= max_operations_in_batch {
            return Ok(out_move_token);
        }
    }

    // Collect any pending requests:
    while let Some((currency, mc_request)) = router_db_client
        .pending_requests_pop_front(friend_public_key.clone())
        .await?
    {
        queue_request(
            router_db_client,
            &mut out_move_token,
            local_public_key,
            friend_public_key.clone(),
            currency,
            mc_request,
            router_output,
        )
        .await?;

        // Make sure we do not exceed maximum amount of operations:
        if out_move_token.len() >= max_operations_in_batch {
            return Ok(out_move_token);
        }
    }

    Ok(out_move_token)
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

/*
// TODO: Rewrite this function to also return index mutations.
// Should be very similar to  `handle_in_move_token_index_mutations`.
/// Attempt to create an outgoing move token
/// May create an empty move token.
pub async fn collect_outgoing_move_token_allow_empty(
    router_db_client: &mut impl RouterDbClient,
    identity_client: &mut IdentityClient,
    local_public_key: &PublicKey,
    friend_public_key: PublicKey,
    max_operations_in_batch: usize,
) -> Result<MoveTokenRequest, RouterError> {
    todo!();
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
*/

/*
/// Like MoveToken, but without the calculated `info_hash` and `signature`
#[derive(Debug)]
struct PreMoveToken {
    pub currencies_operations: CurrenciesOperations,
    pub currencies_diff: Vec<Currency>,
}
*/

/*
/// Like MoveTokenRequest, but wrapping PreMoveToken instead of MoveToken.
#[derive(Debug)]
struct PreMoveTokenRequest {
    pub pre_move_token: PreMoveToken,
    pub token_wanted: bool,
}
*/

/*

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
    local_public_key: &PublicKey,
    friend_public_key: PublicKey,
    max_operations_in_batch: usize,
    router_output: &mut RouterOutput,
) -> Result<Option<OutMoveToken>, RouterError> {
    let out_move_token = collect_currencies_operations(
        router_db_client,
        local_public_key,
        friend_public_key.clone(),
        max_operations_in_batch,
        router_output,
    )
    .await?;

    Ok(if out_move_token.is_empty() {
        // There is nothing interesting to send to remote side
        None
    } else {
        // We have something to send to remote side
        Some(out_move_token)
    })
}


async fn send_pre_move_token(
    router_db_client: &mut impl RouterDbClient,
    identity_client: &mut IdentityClient,
    local_public_key: &PublicKey,
    friend_public_key: PublicKey,
    pre_move_token: PreMoveToken,
) -> Result<MoveToken, RouterError> {
    let move_token = handle_out_move_token(
        router_db_client
            .tc_db_client(friend_public_key.clone())
            .await?
            .ok_or(RouterError::InvalidState)?,
        identity_client,
        pre_move_token.currencies_operations,
        pre_move_token.currencies_diff,
        local_public_key,
        &friend_public_key,
    )
    .await?;

    Ok(move_token)
}
*/

/*
/// Collect all mentioned currencies from a PreMoveToken
fn get_mentioned_currencies(pre_move_token: &PreMoveToken) -> Vec<Currency> {
    // Collect all mentioned currencies:
    let mut currencies = HashSet::new();
    for currency in &pre_move_token.currencies_diff {
        currencies.insert(currency.clone());
    }

    for (currency, _operation) in &pre_move_token.currencies_operations {
        currencies.insert(currency.clone());
    }

    currencies.into_iter().collect()
}
*/

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
            calc_capacities(&currency_info)?
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

fn create_index_mutations(
    friend_public_key: PublicKey,
    currencies_info: &HashMap<Currency, CurrencyInfo>,
) -> Result<Vec<IndexMutation>, RouterError> {
    let mut index_mutations = Vec::new();

    // Sort currencies_info, to have deterministic results
    let currencies_info_vec = {
        let mut currencies_info_vec: Vec<(&Currency, &CurrencyInfo)> = currencies_info
            .iter()
            .map(|(currency, currency_info)| (currency, currency_info))
            .collect();
        currencies_info_vec.sort_by(|(c_a, c_a_inf), (c_b, c_b_inf)| c_a.cmp(c_b));
        currencies_info_vec
    };

    // Create an `IndexMutation` for every currency_info:
    for (currency, currency_info) in currencies_info_vec {
        let (send_capacity, recv_capacity) = calc_capacities(currency_info)?;
        if send_capacity == 0 && recv_capacity == 0 {
            index_mutations.push(IndexMutation::RemoveFriendCurrency(RemoveFriendCurrency {
                public_key: friend_public_key.clone(),
                currency: currency.clone(),
            }));
        } else {
            // UpdateFriendCurrency
            index_mutations.push(IndexMutation::UpdateFriendCurrency(UpdateFriendCurrency {
                public_key: friend_public_key.clone(),
                currency: currency.clone(),
                send_capacity,
                recv_capacity,
                rate: currency_info.rate.clone(),
            }));
        }
    }

    Ok(index_mutations)
}

async fn apply_out_move_token(
    router_db_client: &mut impl RouterDbClient,
    identity_client: &mut IdentityClient,
    local_public_key: &PublicKey,
    friend_public_key: PublicKey,
    out_move_token: OutMoveToken,
) -> Result<(MoveTokenRequest, Vec<IndexMutation>), RouterError> {
    let tc_client = router_db_client
        .tc_db_client(friend_public_key.clone())
        .await?
        .ok_or(RouterError::InvalidState)?;

    // Send move token:
    let (move_token, mentioned_currencies) = out_move_token
        .finalize(
            tc_client,
            identity_client,
            local_public_key,
            &friend_public_key,
        )
        .await?;

    // Get currency info for all mentioned currencies:
    let currencies_info = get_currencies_info(
        router_db_client,
        friend_public_key.clone(),
        &mentioned_currencies,
    )
    .await?;

    // For each currency, create index mutations based on currency information:
    let index_mutations = create_index_mutations(friend_public_key.clone(), &currencies_info)?;

    let move_token_request = MoveTokenRequest {
        move_token,
        token_wanted: is_pending_move_token(router_db_client, friend_public_key).await?,
    };

    Ok((move_token_request, index_mutations))
}

pub async fn handle_out_move_token_index_mutations_disallow_empty(
    router_db_client: &mut impl RouterDbClient,
    identity_client: &mut IdentityClient,
    local_public_key: &PublicKey,
    friend_public_key: PublicKey,
    max_operations_in_batch: usize,
    router_output: &mut RouterOutput,
) -> Result<Option<(MoveTokenRequest, Vec<IndexMutation>)>, RouterError> {
    let out_move_token = collect_currencies_operations(
        router_db_client,
        local_public_key,
        friend_public_key.clone(),
        max_operations_in_batch,
        router_output,
    )
    .await?;

    // Do nothing if we have nothing to send to remote side:
    if out_move_token.is_empty() {
        return Ok(None);
    }

    Ok(Some(
        apply_out_move_token(
            router_db_client,
            identity_client,
            local_public_key,
            friend_public_key,
            out_move_token,
        )
        .await?,
    ))
}

pub async fn handle_out_move_token_index_mutations_allow_empty(
    router_db_client: &mut impl RouterDbClient,
    identity_client: &mut IdentityClient,
    local_public_key: &PublicKey,
    friend_public_key: PublicKey,
    max_operations_in_batch: usize,
    router_output: &mut RouterOutput,
) -> Result<(MoveTokenRequest, Vec<IndexMutation>), RouterError> {
    let out_move_token = collect_currencies_operations(
        router_db_client,
        local_public_key,
        friend_public_key.clone(),
        max_operations_in_batch,
        router_output,
    )
    .await?;

    apply_out_move_token(
        router_db_client,
        identity_client,
        local_public_key,
        friend_public_key,
        out_move_token,
    )
    .await
}

/// Check if we have anything to send to a remove friend on a move token message,
/// without performing any data mutations
pub async fn is_pending_move_token(
    router_db_client: &mut impl RouterDbClient,
    friend_public_key: PublicKey,
) -> Result<bool, RouterError> {
    Ok(is_pending_currencies_operations(router_db_client, friend_public_key.clone()).await?)
}

pub async fn handle_in_move_token_index_mutations<RC>(
    router_db_client: &mut RC,
    identity_client: &mut IdentityClient,
    move_token: MoveToken,
    local_public_key: &PublicKey,
    friend_public_key: PublicKey,
) -> Result<(ReceiveMoveTokenOutput, Vec<IndexMutation>), RouterError>
where
    RC: RouterDbClient,
    RC::TcDbClient: Transaction + Send,
    <RC::TcDbClient as TcDbClient>::McDbClient: Send,
{
    // Handle incoming move token:
    let receive_move_token_output = handle_in_move_token(
        router_db_client
            .tc_db_client(friend_public_key.clone())
            .await?
            .ok_or(RouterError::InvalidState)?,
        identity_client,
        move_token,
        local_public_key,
        &friend_public_key,
    )
    .await?;

    // Get list of mentioned currencies:
    let mentioned_currencies = if let ReceiveMoveTokenOutput::Received(move_token_received) =
        &receive_move_token_output
    {
        let mentioned_currencies_set: HashSet<_> = move_token_received
            .incoming_messages
            .iter()
            .map(|(currency, _incoming_message)| currency)
            .cloned()
            .collect();
        let mut mentioned_currencies_vec: Vec<_> = mentioned_currencies_set.into_iter().collect();
        // Sort currencies to make sure we get deterministic results:
        mentioned_currencies_vec.sort();
        mentioned_currencies_vec
    } else {
        Vec::new()
    };

    // Get currency info for all mentioned currencies:
    let currencies_info = get_currencies_info(
        router_db_client,
        friend_public_key.clone(),
        &mentioned_currencies,
    )
    .await?;

    // For each currency, create index mutations based on currency information:
    let index_mutations = create_index_mutations(friend_public_key.clone(), &currencies_info)?;

    Ok((receive_move_token_output, index_mutations))
}
