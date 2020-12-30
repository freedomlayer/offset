use std::collections::{HashMap, HashSet};

use futures::StreamExt;

use derive_more::From;

use common::async_rpc::OpError;
use common::safe_arithmetic::{SafeSignedArithmetic, SafeUnsignedArithmetic};

use identity::IdentityClient;

use proto::app_server::messages::RelayAddress;
use proto::crypto::PublicKey;
use proto::funder::messages::{
    CancelSendFundsOp, CurrenciesOperations, Currency, CurrencyOperations, FriendMessage,
    FriendTcOp, MoveToken, MoveTokenRequest, RelaysUpdate, RequestSendFundsOp, ResponseSendFundsOp,
};
use proto::index_server::messages::{IndexMutation, RemoveFriendCurrency, UpdateFriendCurrency};

use crate::route::Route;
use crate::router::types::{BackwardsOp, CurrencyInfo, RouterDbClient, RouterOutput, RouterState};
use crate::token_channel::{handle_out_move_token, TcDbClient, TcStatus, TokenChannelError};

#[derive(Debug, From)]
pub enum RouterError {
    FriendAlreadyOnline,
    FriendAlreadyOffline,
    GenerationOverflow,
    BalanceOverflow,
    InvalidRoute,
    UnexpectedTcStatus,
    TokenChannelError(TokenChannelError),
    OpError(OpError),
}

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
/// Return Ok(None) if we have nothing to send
async fn collect_outgoing_move_token(
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
async fn is_pending_move_token(
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

/// Calculate receive capacity for a certain currency
/// This is the number we are going to report to an index server
fn calc_recv_capacity(currency_info: &CurrencyInfo) -> Result<u128, RouterError> {
    if !currency_info.is_open {
        return Ok(0);
    }

    let info_local = if let Some(info_local) = &currency_info.opt_local {
        info_local
    } else {
        return Ok(0);
    };

    let mc_balance = if let Some(mc_balance) = &info_local.opt_remote {
        mc_balance
    } else {
        return Ok(0);
    };

    Ok(currency_info.remote_max_debt.saturating_sub_signed(
        mc_balance
            .balance
            .checked_add_unsigned(mc_balance.remote_pending_debt)
            .ok_or(RouterError::BalanceOverflow)?,
    ))
}

/// Create one update index mutation, based on a given currency info.
fn create_update_index_mutation(
    friend_public_key: PublicKey,
    currency_info: CurrencyInfo,
) -> Result<IndexMutation, RouterError> {
    let recv_capacity = calc_recv_capacity(&currency_info)?;
    Ok(IndexMutation::UpdateFriendCurrency(UpdateFriendCurrency {
        public_key: friend_public_key,
        currency: currency_info.currency,
        recv_capacity,
        rate: currency_info.rate,
    }))
}

/// Create a list of index mutations based on a MoveToken message.
async fn create_index_mutations_from_move_token(
    router_db_client: &mut impl RouterDbClient,
    friend_public_key: PublicKey,
    move_token: &MoveToken,
) -> Result<Vec<IndexMutation>, RouterError> {
    // Collect all mentioned currencies:
    let currencies = {
        let mut currencies = HashSet::new();
        for currency in &move_token.currencies_diff {
            currencies.insert(currency.clone());
        }

        for currency in move_token.currencies_operations.keys() {
            currencies.insert(currency.clone());
        }
        currencies
    };

    let mut index_mutations = Vec::new();

    // Create all index mutations:
    for currency in currencies.into_iter() {
        let opt_currency_info = router_db_client
            .get_currency_info(friend_public_key.clone(), currency.clone())
            .await?;
        if let Some(currency_info) = opt_currency_info {
            // Currency exists
            index_mutations.push(create_update_index_mutation(
                friend_public_key.clone(),
                currency_info,
            )?);
        } else {
            // Currency does not exist anymore
            index_mutations.push(IndexMutation::RemoveFriendCurrency(RemoveFriendCurrency {
                public_key: friend_public_key.clone(),
                currency,
            }));
        }
    }

    Ok(index_mutations)
}

pub async fn set_friend_online(
    router_db_client: &mut impl RouterDbClient,
    router_state: &mut RouterState,
    identity_client: &mut IdentityClient,
    local_public_key: &PublicKey,
    friend_public_key: PublicKey,
    max_operations_in_batch: usize,
) -> Result<RouterOutput, RouterError> {
    if router_state.liveness.is_online(&friend_public_key) {
        // The friend is already marked as online!
        return Err(RouterError::FriendAlreadyOnline);
    }
    router_state.liveness.set_online(friend_public_key.clone());

    let mut output = RouterOutput::new();

    // Check if we have any relays information to send to the remote side:
    if let (Some(generation), relays) = router_db_client
        .get_sent_relays(friend_public_key.clone())
        .await?
    {
        // Add a message for sending relays:
        output.add_friend_message(
            friend_public_key.clone(),
            FriendMessage::RelaysUpdate(RelaysUpdate { generation, relays }),
        );
    }

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
                output.add_friend_message(
                    friend_public_key.clone(),
                    FriendMessage::MoveTokenRequest(move_token_request),
                );
            }
        }
        TcStatus::ConsistentOut(move_token_out, _opt_move_token_hashed_in) => {
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
        TcStatus::Inconsistent(local_reset_terms, _opt_remote_reset_terms) => {
            // Resend reset terms
            output.add_friend_message(
                friend_public_key.clone(),
                FriendMessage::InconsistencyError(local_reset_terms),
            );
        }
    }

    // Add an index mutation for all open currencies:
    let mut open_currencies = router_db_client.list_open_currencies(friend_public_key.clone());
    while let Some(res) = open_currencies.next().await {
        let open_currency = res?;
        output.add_index_mutation(create_update_index_mutation(
            friend_public_key.clone(),
            open_currency,
        )?);
    }

    Ok(output)
}

pub async fn set_friend_offline(
    router_db_client: &mut impl RouterDbClient,
    router_state: &mut RouterState,
    friend_public_key: PublicKey,
) -> Result<RouterOutput, RouterError> {
    if !router_state.liveness.is_online(&friend_public_key) {
        // The friend is already marked as offline!
        return Err(RouterError::FriendAlreadyOffline);
    }
    router_state.liveness.set_offline(&friend_public_key);

    let mut output = RouterOutput::new();

    // Cancel all pending user requests
    while let Some((_currency, pending_user_request)) = router_db_client
        .pending_user_requests_pop_front(friend_public_key.clone())
        .await?
    {
        // Send outgoing cancel to user:
        output.add_incoming_cancel(CancelSendFundsOp {
            request_id: pending_user_request.request_id,
        });
    }

    // Cancel all pending requests
    while let Some((currency, pending_request)) = router_db_client
        .pending_requests_pop_front(friend_public_key.clone())
        .await?
    {
        // Find from which friend this pending request has originated from.
        // Due to inconsistencies, it is possible that this pending request has no origin.
        let opt_request_origin = router_db_client
            .get_remote_pending_request_origin(pending_request.request_id.clone())
            .await?;

        if let Some(request_origin) = opt_request_origin {
            // Cancel request by queue-ing a cancel into the relevant friend's queue:
            router_db_client
                .pending_backwards_push_back(
                    request_origin.friend_public_key,
                    request_origin.currency,
                    BackwardsOp::Cancel(CancelSendFundsOp {
                        request_id: pending_request.request_id,
                    }),
                )
                .await?;
        }
    }

    // Add index mutations
    // We send index mutations to remove all currencies that are considered open
    let mut open_currencies = router_db_client.list_open_currencies(friend_public_key.clone());
    while let Some(res) = open_currencies.next().await {
        let open_currency = res?;
        output.add_index_mutation(IndexMutation::RemoveFriendCurrency(RemoveFriendCurrency {
            public_key: friend_public_key.clone(),
            currency: open_currency.currency,
        }));
    }

    Ok(output)
}

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

pub async fn add_currency(
    router_db_client: &mut impl RouterDbClient,
    friend_public_key: PublicKey,
    currency: Currency,
    identity_client: &mut IdentityClient,
    local_public_key: &PublicKey,
    max_operations_in_batch: usize,
) -> Result<RouterOutput, RouterError> {
    let mut output = RouterOutput::new();
    router_db_client
        .add_currency_config(friend_public_key.clone(), currency)
        .await?;

    flush_friend(
        router_db_client,
        friend_public_key,
        identity_client,
        local_public_key,
        max_operations_in_batch,
        &mut output,
    )
    .await?;
    Ok(output)
}

pub async fn set_remove_currency(
    router_db_client: &mut impl RouterDbClient,
    friend_public_key: PublicKey,
    currency: Currency,
    identity_client: &mut IdentityClient,
    local_public_key: &PublicKey,
    max_operations_in_batch: usize,
) -> Result<RouterOutput, RouterError> {
    let mut output = RouterOutput::new();
    router_db_client
        .set_currency_remove(friend_public_key.clone(), currency)
        .await?;

    flush_friend(
        router_db_client,
        friend_public_key,
        identity_client,
        local_public_key,
        max_operations_in_batch,
        &mut output,
    )
    .await?;
    Ok(output)
}

pub async fn unset_remove_currency(
    router_db_client: &mut impl RouterDbClient,
    friend_public_key: PublicKey,
    currency: Currency,
    identity_client: &mut IdentityClient,
    local_public_key: &PublicKey,
    max_operations_in_batch: usize,
) -> Result<RouterOutput, RouterError> {
    let mut output = RouterOutput::new();
    router_db_client
        .unset_currency_remove(friend_public_key.clone(), currency)
        .await?;

    flush_friend(
        router_db_client,
        friend_public_key,
        identity_client,
        local_public_key,
        max_operations_in_batch,
        &mut output,
    )
    .await?;
    Ok(output)
}

pub async fn set_remote_max_debt(
    router_db_client: &mut impl RouterDbClient,
    friend_public_key: PublicKey,
    currency: Currency,
    remote_max_debt: u128,
) -> Result<RouterOutput, RouterError> {
    // TODO: What to do if currency does not exist?
    // Do we need to somehow report back?

    let mut output = RouterOutput::new();

    let opt_currency_info = router_db_client
        .get_currency_info(friend_public_key.clone(), currency.clone())
        .await?;
    if let Some(currency_info) = opt_currency_info {
        // Currency exists (We don't do anything otherwise)
        // Set remote max debt:
        router_db_client
            .set_remote_max_debt(friend_public_key.clone(), currency.clone(), remote_max_debt)
            .await?;

        // Create an index mutation if needed:
        if currency_info.is_open {
            // Currency is open:
            output.add_index_mutation(create_update_index_mutation(
                friend_public_key.clone(),
                currency_info,
            )?);
        }
    }

    Ok(output)
}

pub async fn open_currency(
    router_db_client: &mut impl RouterDbClient,
    friend_public_key: PublicKey,
    currency: Currency,
) -> Result<RouterOutput, RouterError> {
    let mut output = RouterOutput::new();

    let opt_currency_info = router_db_client
        .get_currency_info(friend_public_key.clone(), currency.clone())
        .await?;

    if let Some(currency_info) = opt_currency_info {
        // Currency exists:
        if !currency_info.is_open {
            // currency is closed:

            // Open currency:
            router_db_client
                .open_currency(friend_public_key.clone(), currency)
                .await?;

            // Add index mutation:
            output.add_index_mutation(create_update_index_mutation(
                friend_public_key,
                currency_info,
            )?);
        }
    }

    Ok(output)
}

pub async fn close_currency(
    router_db_client: &mut impl RouterDbClient,
    friend_public_key: PublicKey,
    currency: Currency,
) -> Result<RouterOutput, RouterError> {
    let mut output = RouterOutput::new();

    let opt_currency_info = router_db_client
        .get_currency_info(friend_public_key.clone(), currency.clone())
        .await?;

    if let Some(currency_info) = opt_currency_info {
        // Currency exists:
        if currency_info.is_open {
            // currency is open:

            // Close currency:
            router_db_client
                .close_currency(friend_public_key.clone(), currency.clone())
                .await?;

            // Add index mutation:
            output.add_index_mutation(IndexMutation::RemoveFriendCurrency(RemoveFriendCurrency {
                public_key: friend_public_key,
                currency,
            }));
        }
    }

    Ok(output)
}

pub async fn update_local_relays(
    _router_db_client: &mut impl RouterDbClient,
    _local_relays: HashMap<PublicKey, RelayAddress>,
) -> Result<RouterOutput, RouterError> {
    // TODO
    // - Pick an internal u128 port for every friend for every relay.
    //      - This info is saved in the db
    // - What should we report outside?
    todo!();
}

pub async fn incoming_friend_message(
    _router_db_client: &mut impl RouterDbClient,
    _friend_public_key: PublicKey,
    _friend_message: FriendMessage,
) -> Result<RouterOutput, RouterError> {
    // TODO
    todo!();
}
