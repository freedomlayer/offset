use derive_more::From;
use std::collections::{HashMap, HashSet};

use common::async_rpc::OpError;

use identity::IdentityClient;

use proto::app_server::messages::RelayAddress;
use proto::crypto::PublicKey;
use proto::funder::messages::{
    CancelSendFundsOp, Currency, CurrencyOperations, FriendMessage, FriendTcOp, MoveToken,
    MoveTokenRequest, RelaysUpdate, RequestSendFundsOp, ResponseSendFundsOp,
};

use crate::switch::types::{BackwardsOp, SwitchDbClient, SwitchOutput, SwitchState};
use crate::token_channel::{handle_out_move_token, TcDbClient, TcStatus, TokenChannelError};

#[derive(Debug, From)]
pub enum SwitchError {
    FriendAlreadyOnline,
    GenerationOverflow,
    TokenChannelError(TokenChannelError),
    OpError(OpError),
}

fn operations_vec_to_currencies_operations(
    operations_vec: Vec<(Currency, FriendTcOp)>,
) -> Vec<CurrencyOperations> {
    let mut operations_map = HashMap::<Currency, Vec<FriendTcOp>>::new();
    for (currency, tc_op) in operations_vec {
        let entry = operations_map.entry(currency).or_insert(Vec::new());
        (*entry).push(tc_op);
    }

    // Sort by currency, for deterministic results:
    let mut currencies_operations: Vec<CurrencyOperations> = operations_map
        .into_iter()
        .map(|(currency, operations)| CurrencyOperations {
            currency,
            operations,
        })
        .collect();
    currencies_operations.sort_by(|co_a, co_b| co_a.currency.cmp(&co_b.currency));
    currencies_operations
}

async fn collect_currencies_operations(
    switch_db_client: &mut impl SwitchDbClient,
    friend_public_key: PublicKey,
    max_operations_in_batch: usize,
) -> Result<Vec<CurrencyOperations>, SwitchError> {
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

    Ok(output)
}

pub async fn set_friend_offline(
    _switch_db_client: &mut impl SwitchDbClient,
    _switch_state: &mut SwitchState,
    _friend_public_key: PublicKey,
) -> Result<SwitchOutput, SwitchError> {
    todo!();
}

pub async fn send_request(
    _switch_db_client: &mut impl SwitchDbClient,
    _request: RequestSendFundsOp,
) -> Result<SwitchOutput, SwitchError> {
    // TODO:
    // - Add request to relevant user pending requests queue (According to friend on route)
    // - For the relevant friend: If token is present:
    //      - Compose a friend move token message
    // - If the token is not present:
    //      - Compose a request token message.
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
