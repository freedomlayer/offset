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
        .get_last_sent_relays(friend_public_key.clone())
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

/// Calculate max generation
/// Returns None if no generation was found: This would mean that the remote friend is fully
/// synced.
fn max_generation(sent_relays: &[SentRelay]) -> Option<u128> {
    let mut opt_max_generation = None;
    for sent_relay in sent_relays {
        if let Some(sent_relay_generation) = &sent_relay.opt_generation {
            if let Some(max_generation) = &mut opt_max_generation {
                if sent_relay_generation > max_generation {
                    *max_generation = *sent_relay_generation;
                }
            } else {
                opt_max_generation = Some(*sent_relay_generation);
            }
        }
    }
    opt_max_generation
}

/// Returns whether friend's relays were updated
async fn update_friend_local_relays(
    router_db_client: &mut impl RouterDbClient,
    local_relays: HashMap<PublicKey, NetAddress>,
    friend_public_key: PublicKey,
    rng: &mut impl CryptoRandom,
) -> Result<Option<(u128, Vec<RelayAddressPort>)>, RouterError> {
    let sent_relays = router_db_client
        .get_sent_relays(friend_public_key.clone())
        .await?;

    let opt_max_generation = max_generation(&sent_relays);
    let new_generation = match opt_max_generation {
        Some(g) => g.checked_add(1).ok_or(RouterError::GenerationOverflow)?,
        None => 0,
    };

    // Update local relays:
    let mut new_sent_relays = Vec::new();
    let mut sent_relays_updated: bool = false;
    let mut c_local_relays = local_relays.clone();
    for mut sent_relay in sent_relays.into_iter() {
        if let Some(relay_address) = c_local_relays.remove(&sent_relay.relay_address.public_key) {
            // We should update this relay:
            let new_sent_relay = SentRelay {
                relay_address: sent_relay.relay_address.clone(),
                is_remove: false,
                opt_generation: sent_relay.opt_generation, // ??
            };
            sent_relays_updated |= sent_relay != new_sent_relay;
            sent_relay.is_remove = false;
        } else {
            // We should remove this relay from sent_relays:
            if !sent_relay.is_remove {
                sent_relay.is_remove = true;
                sent_relay.opt_generation = Some(new_generation);
                sent_relays_updated = true;
            }
            new_sent_relays.push(sent_relay);
        }
    }

    // Add remaining local relays:
    for (relay_public_key, address) in c_local_relays.into_iter() {
        let relay_address = RelayAddressPort {
            public_key: relay_public_key,
            address,
            // Randomly generate a new port:
            port: NodePort::rand_gen(rng),
        };
        new_sent_relays.push(SentRelay {
            relay_address,
            is_remove: false,
            opt_generation: Some(new_generation),
        });
        sent_relays_updated = true;
    }

    Ok(if sent_relays_updated {
        // Update sent relays:
        router_db_client
            .set_sent_relays(friend_public_key.clone(), new_sent_relays.clone())
            .await?;
        Some((
            new_generation,
            new_sent_relays
                .into_iter()
                .map(|sent_relay| sent_relay.relay_address)
                .collect(),
        ))
    } else {
        None
    })
}

pub async fn update_local_relays(
    router_db_client: &mut impl RouterDbClient,
    router_state: &RouterState,
    local_relays: HashMap<PublicKey, NetAddress>,
    rng: &mut impl CryptoRandom,
) -> Result<RouterOutput, RouterError> {
    let mut output = RouterOutput::new();

    let mut opt_friend_public_key = None;
    while let Some(friend_public_key) = router_db_client
        .get_next_friend(opt_friend_public_key)
        .await?
    {
        opt_friend_public_key = Some(friend_public_key.clone());

        let opt_res = update_friend_local_relays(
            router_db_client,
            local_relays.clone(),
            friend_public_key.clone(),
            rng,
        )
        .await?;

        // If sent_relays was updated and the friend is ready, we need to send relays to
        // remote friend:
        if let Some((generation, relays)) = opt_res {
            // Make sure that remote friend is online:
            if router_state.liveness.is_online(&friend_public_key) {
                // Add a message for sending relays:
                output.add_friend_message(
                    friend_public_key.clone(),
                    FriendMessage::RelaysUpdate(RelaysUpdate { generation, relays }),
                );
            }
        }
    }

    Ok(output)
}

async fn incoming_move_token_request<RC>(
    mut router_db_client: &mut RC,
    router_state: &mut RouterState,
    friend_public_key: PublicKey,
    identity_client: &mut IdentityClient,
    local_public_key: &PublicKey,
    max_operations_in_batch: usize,
    move_token_request: MoveTokenRequest,
) -> Result<RouterOutput, RouterError>
where
    RC: RouterDbClient,
    RC::TcDbClient: Transaction + Send,
    <RC::TcDbClient as TcDbClient>::McDbClient: Send,
{
    let mut output = RouterOutput::new();
    let receive_move_token_output = handle_in_move_token(
        router_db_client.tc_db_client(friend_public_key.clone()),
        identity_client,
        move_token_request.move_token,
        local_public_key,
        &friend_public_key,
    )
    .await?;

    match receive_move_token_output {
        ReceiveMoveTokenOutput::Duplicate => {
            // We should have nothing else to send at this point, otherwise we would have already
            // sent it.
            assert!(!is_pending_move_token(router_db_client, friend_public_key.clone()).await?);

            // Possibly send token to remote side (According to token_wanted)
            if move_token_request.token_wanted {
                let out_move_token_request = collect_outgoing_move_token_allow_empty(
                    router_db_client,
                    identity_client,
                    local_public_key,
                    friend_public_key.clone(),
                    max_operations_in_batch,
                )
                .await?;

                // TODO: Should we really check for liveness here? We just got a message from this
                // friend. Think about liveness design here. What if we ever forget to check for
                // liveness before adding a friend message? Maybe this should be checked in
                // different way, or a different layer?
                if router_state.liveness.is_online(&friend_public_key) {
                    output.add_friend_message(
                        friend_public_key.clone(),
                        FriendMessage::MoveTokenRequest(out_move_token_request),
                    );
                }
            }
        }
        ReceiveMoveTokenOutput::RetransmitOutgoing(move_token) => todo!(),
        ReceiveMoveTokenOutput::Received(move_token_received) => todo!(),
        ReceiveMoveTokenOutput::ChainInconsistent(reset_terms) => todo!(), // (local_reset_token, local_reset_move_token_counter)
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
