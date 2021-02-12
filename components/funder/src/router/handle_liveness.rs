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
    BackwardsOp, CurrencyInfo, RouterControl, RouterDbClient, RouterError, RouterInfo,
    RouterOutput, RouterState, SentRelay,
};

use crate::mutual_credit::McCancel;

use crate::token_channel::{
    handle_in_move_token, ReceiveMoveTokenOutput, TcDbClient, TcStatus, TokenChannelError,
};

use crate::router::utils::index_mutation::create_index_mutation;
use crate::router::utils::move_token::{
    handle_out_move_token_index_mutations_disallow_empty, is_pending_move_token,
};

pub async fn set_friend_online(
    control: &mut impl RouterControl,
    info: &RouterInfo,
    friend_public_key: PublicKey,
) -> Result<(), RouterError> {
    let mut access = control.access();

    // First we make sure that the friend exists:
    let tc_status = if let Some(tc_db_client) = access
        .router_db_client
        .tc_db_client(friend_public_key.clone())
        .await?
    {
        tc_db_client.get_tc_status().await?
    } else {
        return Ok(());
    };

    if access.ephemeral.liveness.is_online(&friend_public_key) {
        // The friend is already marked as online!
        return Err(RouterError::FriendAlreadyOnline);
    }
    access
        .ephemeral
        .liveness
        .set_online(friend_public_key.clone());

    // Check if we have any relays information to send to the remote side:
    if let (Some(generation), relays) = access
        .router_db_client
        .get_last_sent_relays(friend_public_key.clone())
        .await?
    {
        // Add a message for sending relays:
        access.output.add_friend_message(
            friend_public_key.clone(),
            FriendMessage::RelaysUpdate(RelaysUpdate { generation, relays }),
        );
    }

    match tc_status {
        TcStatus::ConsistentIn(_) => {
            // Create an outgoing move token if we have something to send.
            let opt_tuple = handle_out_move_token_index_mutations_disallow_empty(
                access.router_db_client,
                access.identity_client,
                &info.local_public_key,
                friend_public_key.clone(),
                info.max_operations_in_batch,
                &mut access.output,
            )
            .await?;

            if let Some((move_token_request, _index_mutations)) = opt_tuple {
                // We discard index_mutations calculation here, because we are going to add all
                // open currencies anyways.

                // We have something to send to remote side:
                access.output.add_friend_message(
                    friend_public_key.clone(),
                    FriendMessage::MoveTokenRequest(move_token_request),
                );
            }
        }
        TcStatus::ConsistentOut(move_token_out, _opt_move_token_hashed_in) => {
            // Resend outgoing move token,
            // possibly asking for the token if we have something to send
            access.output.add_friend_message(
                friend_public_key.clone(),
                FriendMessage::MoveTokenRequest(MoveTokenRequest {
                    move_token: move_token_out,
                    token_wanted: is_pending_move_token(
                        access.router_db_client,
                        friend_public_key.clone(),
                    )
                    .await?,
                }),
            );
        }
        TcStatus::Inconsistent(local_reset_terms, _opt_remote_reset_terms) => {
            // Resend reset terms
            access.output.add_friend_message(
                friend_public_key.clone(),
                FriendMessage::InconsistencyError(local_reset_terms),
            );
        }
    }

    // Add an index mutation for all open currencies:
    let mut open_currencies = access
        .router_db_client
        .list_open_currencies(friend_public_key.clone());
    while let Some(res) = open_currencies.next().await {
        let (open_currency, open_currency_info) = res?;

        let index_mutation =
            create_index_mutation(friend_public_key.clone(), open_currency, open_currency_info)?;

        // We only add currencies with non zero send/recv capacity
        if matches!(IndexMutation::UpdateFriendCurrency, index_mutation) {
            access.output.add_index_mutation(index_mutation);
        }
    }

    Ok(())
}

pub async fn set_friend_offline(
    control: &mut impl RouterControl,
    friend_public_key: PublicKey,
) -> Result<(), RouterError> {
    let access = control.access();

    // First we make sure that the friend exists:
    let mut output = RouterOutput::new();
    if access
        .router_db_client
        .tc_db_client(friend_public_key.clone())
        .await?
        .is_none()
    {
        return Ok(());
    }

    if !access.ephemeral.liveness.is_online(&friend_public_key) {
        // The friend is already marked as offline!
        return Err(RouterError::FriendAlreadyOffline);
    }
    access.ephemeral.liveness.set_offline(&friend_public_key);

    // Cancel all pending user requests
    while let Some((currency, pending_user_request)) = access
        .router_db_client
        .pending_user_requests_pop_front(friend_public_key.clone())
        .await?
    {
        // Clear the request from local requests list
        access
            .router_db_client
            .remove_local_request(pending_user_request.request_id.clone())
            .await?;
        // Send outgoing cancel to user:
        access.output.add_incoming_cancel(
            currency,
            McCancel {
                request_id: pending_user_request.request_id,
            },
        );
    }

    // Cancel all pending requests
    while let Some((currency, pending_request)) = access
        .router_db_client
        .pending_requests_pop_front(friend_public_key.clone())
        .await?
    {
        // Find from which friend this pending request has originated from.
        // Due to inconsistencies, it is possible that this pending request has no origin (An
        // orphan request)
        let opt_request_origin = access
            .router_db_client
            .get_remote_pending_request_origin(pending_request.request_id.clone())
            .await?;

        if let Some(request_origin) = opt_request_origin {
            // Currency should be the same!
            if currency != request_origin.currency {
                return Err(RouterError::InvalidState);
            }

            // Cancel request by queue-ing a cancel into the relevant friend's queue:
            access
                .router_db_client
                .pending_backwards_push_back(
                    request_origin.friend_public_key,
                    BackwardsOp::Cancel(
                        request_origin.currency,
                        McCancel {
                            request_id: pending_request.request_id,
                        },
                    ),
                )
                .await?;
        }
    }

    // Add index mutations
    // We send index mutations to remove all currencies that are considered open
    let mut open_currencies = access
        .router_db_client
        .list_open_currencies(friend_public_key.clone());
    while let Some(res) = open_currencies.next().await {
        let (open_currency, _open_currency_info) = res?;
        output.add_index_mutation(IndexMutation::RemoveFriendCurrency(RemoveFriendCurrency {
            public_key: friend_public_key.clone(),
            currency: open_currency,
        }));
    }

    Ok(())
}
