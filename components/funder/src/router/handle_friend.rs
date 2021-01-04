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

use crate::mutual_credit::incoming::{
    IncomingCancelSendFundsOp, IncomingMessage, IncomingResponseSendFundsOp,
};
use crate::router::utils::flush::flush_friend;
use crate::router::utils::index_mutation::{
    create_index_mutations_from_move_token, create_update_index_mutation,
};
use crate::router::utils::move_token::{
    collect_outgoing_move_token, collect_outgoing_move_token_allow_empty, is_pending_move_token,
};
use crate::token_channel::{
    handle_in_move_token, ReceiveMoveTokenOutput, TcDbClient, TcStatus, TokenChannelError,
};

async fn incoming_message_request<RC>(
    mut router_db_client: &mut RC,
    router_state: &mut RouterState,
    friend_public_key: PublicKey,
    identity_client: &mut IdentityClient,
    local_public_key: &PublicKey,
    max_operations_in_batch: usize,
    router_output: &mut RouterOutput,
    currency: Currency,
    mut request_send_funds: RequestSendFundsOp,
) -> Result<(), RouterError>
where
    RC: RouterDbClient,
{
    // - If directed to us, punt
    // - If directed to another friend:
    //      - If friend exists and ready, forward to next hop
    //      - Otherwise, queue cancel
    if request_send_funds.route.is_empty() {
        // Directed to us. Punt:
        router_output.add_incoming_request(request_send_funds);
    } else {
        // Route is not empty. We need to forward the request to a friend
        let next_public_key = request_send_funds.route.remove(0);

        // Check if next public key corresponds to a friend that is ready
        let is_ready = if let Some(tc_db_client) = router_db_client
            .tc_db_client(next_public_key.clone())
            .await?
        {
            match tc_db_client.get_tc_status().await? {
                TcStatus::ConsistentIn(..) | TcStatus::ConsistentOut(..) => true,
                TcStatus::Inconsistent(..) => false,
            }
        } else {
            // next public key points to a nonexistent friend!
            false
        } && router_state.liveness.is_online(&next_public_key);

        if is_ready {
            // Queue request to friend and flush destination friend
            router_db_client
                .pending_requests_push_back(next_public_key.clone(), currency, request_send_funds)
                .await?;
            flush_friend(
                router_db_client,
                next_public_key,
                identity_client,
                local_public_key,
                max_operations_in_batch,
                router_output,
            )
            .await?;
        } else {
            // Return a cancel message and flush origin friend
            router_db_client
                .pending_backwards_push_back(
                    friend_public_key.clone(),
                    currency,
                    BackwardsOp::Cancel(CancelSendFundsOp {
                        request_id: request_send_funds.request_id.clone(),
                    }),
                )
                .await?;

            flush_friend(
                router_db_client,
                friend_public_key,
                identity_client,
                local_public_key,
                max_operations_in_batch,
                router_output,
            )
            .await?;
        }
    }
    Ok(())
}

async fn incoming_message_request_cancel<RC>(
    mut router_db_client: &mut RC,
    router_state: &mut RouterState,
    friend_public_key: PublicKey,
    identity_client: &mut IdentityClient,
    router_output: &mut RouterOutput,
    request_send_funds: RequestSendFundsOp,
) -> Result<(), RouterError>
where
    RC: RouterDbClient,
    // TODO: Maybe not necessary:
    RC::TcDbClient: Transaction + Send,
    <RC::TcDbClient as TcDbClient>::McDbClient: Send,
{
    todo!();
}

async fn incoming_message_response<RC>(
    mut router_db_client: &mut RC,
    router_state: &mut RouterState,
    friend_public_key: PublicKey,
    identity_client: &mut IdentityClient,
    router_output: &mut RouterOutput,
    incoming_response: IncomingResponseSendFundsOp,
) -> Result<(), RouterError>
where
    RC: RouterDbClient,
    RC::TcDbClient: Transaction + Send,
    <RC::TcDbClient as TcDbClient>::McDbClient: Send,
{
    todo!();
}

async fn incoming_message_cancel<RC>(
    mut router_db_client: &mut RC,
    router_state: &mut RouterState,
    friend_public_key: PublicKey,
    identity_client: &mut IdentityClient,
    router_output: &mut RouterOutput,
    cancel_send_funds: IncomingCancelSendFundsOp,
) -> Result<(), RouterError>
where
    RC: RouterDbClient,
    // TODO: Maybe not necessary:
    RC::TcDbClient: Transaction + Send,
    <RC::TcDbClient as TcDbClient>::McDbClient: Send,
{
    todo!();
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
    // TODO: Maybe not necessary:
    RC::TcDbClient: Transaction + Send,
    <RC::TcDbClient as TcDbClient>::McDbClient: Send,
{
    let mut output = RouterOutput::new();
    let receive_move_token_output = handle_in_move_token(
        router_db_client
            .tc_db_client(friend_public_key.clone())
            .await?
            .ok_or(RouterError::InvalidDbState)?,
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
        ReceiveMoveTokenOutput::RetransmitOutgoing(move_token) => {
            // Retransmit outgoing move token message to friend:
            let move_token_request = MoveTokenRequest {
                move_token,
                token_wanted: is_pending_move_token(router_db_client, friend_public_key.clone())
                    .await?,
            };

            if router_state.liveness.is_online(&friend_public_key) {
                output.add_friend_message(
                    friend_public_key.clone(),
                    FriendMessage::MoveTokenRequest(move_token_request),
                );
            }
        }
        ReceiveMoveTokenOutput::Received(move_token_received) => {
            for (currency, incoming_message) in move_token_received.incoming_messages {
                match incoming_message {
                    IncomingMessage::Request(request_send_funds) => {
                        incoming_message_request(
                            router_db_client,
                            router_state,
                            friend_public_key.clone(),
                            identity_client,
                            local_public_key,
                            max_operations_in_batch,
                            &mut output,
                            currency,
                            request_send_funds,
                        )
                        .await
                    }
                    IncomingMessage::RequestCancel(request_send_funds) => {
                        incoming_message_request_cancel(
                            router_db_client,
                            router_state,
                            friend_public_key.clone(),
                            identity_client,
                            &mut output,
                            request_send_funds,
                        )
                        .await
                    }
                    IncomingMessage::Response(incoming_response) => {
                        incoming_message_response(
                            router_db_client,
                            router_state,
                            friend_public_key.clone(),
                            identity_client,
                            &mut output,
                            incoming_response,
                        )
                        .await
                    }
                    IncomingMessage::Cancel(incoming_cancel) => {
                        incoming_message_cancel(
                            router_db_client,
                            router_state,
                            friend_public_key.clone(),
                            identity_client,
                            &mut output,
                            incoming_cancel,
                        )
                        .await
                    }
                }?;
            }
            todo!();
        }
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
