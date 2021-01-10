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
use crate::router::utils::flush::flush_friend;
use crate::router::utils::index_mutation::{
    create_index_mutations_from_outgoing_move_token, create_update_index_mutation,
};
use crate::router::utils::move_token::{collect_outgoing_move_token, is_pending_move_token};
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
    // First we make sure that the friend exists:
    let mut output = RouterOutput::new();
    let tc_status = if let Some(tc_db_client) = router_db_client
        .tc_db_client(friend_public_key.clone())
        .await?
    {
        tc_db_client.get_tc_status().await?
    } else {
        return Ok(output);
    };

    if router_state.liveness.is_online(&friend_public_key) {
        // The friend is already marked as online!
        return Err(RouterError::FriendAlreadyOnline);
    }
    router_state.liveness.set_online(friend_public_key.clone());

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

    match tc_status {
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
    // First we make sure that the friend exists:
    let mut output = RouterOutput::new();
    if router_db_client
        .tc_db_client(friend_public_key.clone())
        .await?
        .is_none()
    {
        return Ok(output);
    }

    if !router_state.liveness.is_online(&friend_public_key) {
        // The friend is already marked as offline!
        return Err(RouterError::FriendAlreadyOffline);
    }
    router_state.liveness.set_offline(&friend_public_key);

    // Cancel all pending user requests
    while let Some((_currency, pending_user_request)) = router_db_client
        .pending_user_requests_pop_front(friend_public_key.clone())
        .await?
    {
        // Clear the request from local requests list
        router_db_client
            .remove_local_request(pending_user_request.request_id.clone())
            .await?;
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
