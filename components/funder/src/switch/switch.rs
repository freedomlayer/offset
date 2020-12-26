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
use crate::switch::types::{BackwardsOp, CurrencyInfo, SwitchDbClient, SwitchOutput, SwitchState};
use crate::token_channel::{handle_out_move_token, TcDbClient, TcStatus, TokenChannelError};

#[derive(Debug, From)]
pub enum SwitchError {
    FriendAlreadyOnline,
    FriendAlreadyOffline,
    GenerationOverflow,
    BalanceOverflow,
    InvalidRoute,
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
    switch_db_client: &mut impl SwitchDbClient,
    friend_public_key: PublicKey,
    max_operations_in_batch: usize,
) -> Result<CurrenciesOperations, SwitchError> {
    let mut operations_vec = Vec::<(Currency, FriendTcOp)>::new();

    // Collect any pending responses and cancels:
    while let Some((currency, backwards_op)) = switch_db_client
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
    while let Some((currency, request_op)) = switch_db_client
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
    while let Some((currency, request_op)) = switch_db_client
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
    switch_db_client: &mut impl SwitchDbClient,
    friend_public_key: PublicKey,
) -> Result<bool, SwitchError> {
    Ok(!switch_db_client
        .pending_backwards_is_empty(friend_public_key.clone())
        .await?
        || !switch_db_client
            .pending_user_requests_is_empty(friend_public_key.clone())
            .await?
        || !switch_db_client
            .pending_requests_is_empty(friend_public_key.clone())
            .await?)
}

/// Attempt to create an outgoing move token
/// Return Ok(None) if we have nothing to send
async fn collect_outgoing_move_token(
    switch_db_client: &mut impl SwitchDbClient,
    identity_client: &mut IdentityClient,
    local_public_key: &PublicKey,
    friend_public_key: PublicKey,
    max_operations_in_batch: usize,
) -> Result<Option<MoveTokenRequest>, SwitchError> {
    let currencies_operations = collect_currencies_operations(
        switch_db_client,
        friend_public_key.clone(),
        max_operations_in_batch,
    )
    .await?;

    let mut currencies_diff = switch_db_client
        .currencies_diff(friend_public_key.clone())
        .await?;

    Ok(
        if currencies_operations.is_empty() && currencies_diff.is_empty() {
            // There is nothing interesting to send to remote side
            None
        } else {
            // We have something to send to remote side
            let move_token = handle_out_move_token(
                switch_db_client.tc_db_client(friend_public_key.clone()),
                identity_client,
                currencies_operations,
                currencies_diff,
                local_public_key,
                &friend_public_key,
            )
            .await?;
            Some(MoveTokenRequest {
                move_token,
                token_wanted: is_pending_currencies_operations(switch_db_client, friend_public_key)
                    .await?,
            })
        },
    )
}

/// Check if we have anything to send to a remove friend on a move token message,
/// without performing any data mutations
async fn is_pending_move_token(
    switch_db_client: &mut impl SwitchDbClient,
    friend_public_key: PublicKey,
) -> Result<bool, SwitchError> {
    Ok(
        is_pending_currencies_operations(switch_db_client, friend_public_key.clone()).await?
            || !switch_db_client
                .currencies_diff(friend_public_key.clone())
                .await?
                .is_empty(),
    )
}

/// Calculate receive capacity for a certain currency
/// This is the number we are going to report to an index server
fn calc_recv_capacity(currency_info: &CurrencyInfo) -> Result<u128, SwitchError> {
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
            .ok_or(SwitchError::BalanceOverflow)?,
    ))
}

/// Create one update index mutation, based on a given currency info.
fn create_update_index_mutation(
    friend_public_key: PublicKey,
    currency_info: CurrencyInfo,
) -> Result<IndexMutation, SwitchError> {
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
    switch_db_client: &mut impl SwitchDbClient,
    friend_public_key: PublicKey,
    move_token: &MoveToken,
) -> Result<Vec<IndexMutation>, SwitchError> {
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
        let opt_currency_info = switch_db_client
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
    switch_db_client: &mut impl SwitchDbClient,
    switch_state: &mut SwitchState,
    identity_client: &mut IdentityClient,
    local_public_key: &PublicKey,
    friend_public_key: PublicKey,
    max_operations_in_batch: usize,
) -> Result<SwitchOutput, SwitchError> {
    if switch_state.liveness.is_online(&friend_public_key) {
        // The friend is already marked as online!
        return Err(SwitchError::FriendAlreadyOnline);
    }
    switch_state.liveness.set_online(friend_public_key.clone());

    let mut output = SwitchOutput::new();

    // Check if we have any relays information to send to the remote side:
    if let (Some(generation), relays) = switch_db_client
        .get_sent_relays(friend_public_key.clone())
        .await?
    {
        // Add a message for sending relays:
        output.add_friend_message(
            friend_public_key.clone(),
            FriendMessage::RelaysUpdate(RelaysUpdate { generation, relays }),
        );
    }

    match switch_db_client
        .tc_db_client(friend_public_key.clone())
        .get_tc_status()
        .await?
    {
        TcStatus::ConsistentIn(_) => {
            // Create an outgoing move token if we have something to send.
            let opt_move_token_request = collect_outgoing_move_token(
                switch_db_client,
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
                        switch_db_client,
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
    let mut open_currencies = switch_db_client.list_open_currencies(friend_public_key.clone());
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
    switch_db_client: &mut impl SwitchDbClient,
    switch_state: &mut SwitchState,
    friend_public_key: PublicKey,
) -> Result<SwitchOutput, SwitchError> {
    if !switch_state.liveness.is_online(&friend_public_key) {
        // The friend is already marked as offline!
        return Err(SwitchError::FriendAlreadyOffline);
    }
    switch_state.liveness.set_offline(&friend_public_key);

    let mut output = SwitchOutput::new();

    // Cancel all pending user requests
    while let Some((_currency, pending_user_request)) = switch_db_client
        .pending_user_requests_pop_front(friend_public_key.clone())
        .await?
    {
        // Send outgoing cancel to user:
        output.add_incoming_cancel(CancelSendFundsOp {
            request_id: pending_user_request.request_id,
        });
    }

    // Cancel all pending requests
    while let Some((currency, pending_request)) = switch_db_client
        .pending_requests_pop_front(friend_public_key.clone())
        .await?
    {
        // Find from which friend this pending request has originated from:
        let origin_public_key = switch_db_client
            .get_remote_pending_request_friend_public_key(pending_request.request_id.clone())
            .await?;

        // Cancel request by queue-ing a cancel into the relevant friend's queue:
        switch_db_client
            .pending_backwards_push_back(
                origin_public_key,
                currency,
                BackwardsOp::Cancel(CancelSendFundsOp {
                    request_id: pending_request.request_id,
                }),
            )
            .await?;
    }

    // Add index mutations
    // We send index mutations to remove all currencies that are considered open
    let mut open_currencies = switch_db_client.list_open_currencies(friend_public_key.clone());
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
    switch_db_client: &mut impl SwitchDbClient,
    switch_state: &SwitchState,
    currency: Currency,
    mut request: RequestSendFundsOp,
    identity_client: &mut IdentityClient,
    local_public_key: &PublicKey,
    max_operations_in_batch: usize,
) -> Result<SwitchOutput, SwitchError> {
    // Make sure that the provided route is valid:
    if !request.route.is_valid() {
        return Err(SwitchError::InvalidRoute);
    }

    let mut output = SwitchOutput::new();

    // We cut the first two public keys from the route:
    // Pop first route public key:
    let route_local_public_key = request.route.remove(0);
    if &route_local_public_key != local_public_key {
        return Err(SwitchError::InvalidRoute);
    }
    // Pop second route public key:
    let friend_public_key = request.route.remove(0);

    // Gather information about friend's channel and liveness:
    let tc_status = switch_db_client
        .tc_db_client(friend_public_key.clone())
        .get_tc_status()
        .await?;
    let is_online = switch_state.liveness.is_online(&friend_public_key);

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
            switch_db_client
                .pending_user_requests_push_back(friend_public_key.clone(), currency, request)
                .await?;

            // Create an outgoing move token if we have something to send.
            let opt_move_token_request = collect_outgoing_move_token(
                switch_db_client,
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
                    switch_db_client,
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
            switch_db_client
                .pending_user_requests_push_back(friend_public_key.clone(), currency, request)
                .await?;

            // Resend outgoing move token,
            // possibly asking for the token if we have something to send
            output.add_friend_message(
                friend_public_key.clone(),
                FriendMessage::MoveTokenRequest(MoveTokenRequest {
                    move_token: move_token_out,
                    token_wanted: is_pending_move_token(
                        switch_db_client,
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

pub async fn send_response(
    _switch_db_client: &mut impl SwitchDbClient,
    _response: ResponseSendFundsOp,
) -> Result<SwitchOutput, SwitchError> {
    // TODO:
    // - Find relevant request (And so, find relevant friend_public_key)
    // - Queue response
    // - Attempt to collect a MoveToken for the relevant friend.
    todo!();
}

pub async fn send_cancel(
    _switch_db_client: &mut impl SwitchDbClient,
    _cancel: CancelSendFundsOp,
) -> Result<SwitchOutput, SwitchError> {
    todo!();
}

pub async fn add_currency(
    _switch_db_client: &mut impl SwitchDbClient,
    _friend_public_key: PublicKey,
    _currency: Currency,
) -> Result<SwitchOutput, SwitchError> {
    // TODO
    todo!();
}

pub async fn set_remove_currency(
    _switch_db_client: &mut impl SwitchDbClient,
    _friend_public_key: PublicKey,
    _currency: Currency,
) -> Result<SwitchOutput, SwitchError> {
    // TODO
    todo!();
}

pub async fn unset_remove_currency(
    _switch_db_client: &mut impl SwitchDbClient,
    _friend_public_key: PublicKey,
    _currency: Currency,
) -> Result<SwitchOutput, SwitchError> {
    // TODO
    todo!();
}

// TODO: Do we need to send an update to index client somehow?
pub async fn set_remote_max_debt(
    _switch_db_client: &mut impl SwitchDbClient,
    _friend_public_key: PublicKey,
    _currency: Currency,
    _remote_max_debt: u128,
) -> Result<SwitchOutput, SwitchError> {
    // TODO
    todo!();
}

// TODO: Do we need to send an update to index client somehow?
pub async fn open_currency(
    _switch_db_client: &mut impl SwitchDbClient,
    _friend_public_key: PublicKey,
    _currency: Currency,
) -> Result<SwitchOutput, SwitchError> {
    // TODO
    todo!();
}

// TODO: Do we need to send an update to index client somehow?
pub async fn close_currency(
    _switch_db_client: &mut impl SwitchDbClient,
    _friend_public_key: PublicKey,
    _currency: Currency,
) -> Result<SwitchOutput, SwitchError> {
    // TODO
    todo!();
}

pub async fn update_local_relays(
    _switch_db_client: &mut impl SwitchDbClient,
    _local_relays: HashMap<PublicKey, RelayAddress>,
) -> Result<SwitchOutput, SwitchError> {
    // TODO
    todo!();
}

pub async fn incoming_friend_message(
    _switch_db_client: &mut impl SwitchDbClient,
    _friend_public_key: PublicKey,
    _friend_message: FriendMessage,
) -> Result<SwitchOutput, SwitchError> {
    // TODO
    todo!();
}
