use std::collections::{HashMap, HashSet};

use futures::StreamExt;

use derive_more::From;

use common::async_rpc::OpError;
use common::safe_arithmetic::{SafeSignedArithmetic, SafeUnsignedArithmetic};

use identity::IdentityClient;

use proto::app_server::messages::RelayAddressPort;
use proto::crypto::{NodePort, PublicKey};
use proto::funder::messages::{
    CancelSendFundsOp, Currency, FriendMessage, FriendTcOp, MoveToken, MoveTokenRequest,
    RelaysUpdate, RequestSendFundsOp, ResponseSendFundsOp,
};
use proto::index_server::messages::{IndexMutation, RemoveFriendCurrency, UpdateFriendCurrency};
use proto::net::messages::NetAddress;

use crypto::rand::{CryptoRandom, RandGen};

use crate::token_channel::{TcDbClient, TcStatus, TokenChannelError};

use crate::route::Route;
use crate::router::types::{
    BackwardsOp, CurrencyInfo, RouterDbClient, RouterError, RouterOutput, RouterState, SentRelay,
};
use crate::router::utils::index_mutation::create_index_mutations_from_outgoing_move_token;
use crate::router::utils::move_token::{
    handle_out_move_token_index_mutations_disallow_empty, is_pending_move_token,
};

/// Attempt to send as much as possible through a token channel to remote side
/// Assumes that the token channel is in consistent state (Incoming / Outgoing).
pub async fn flush_friend(
    router_db_client: &mut impl RouterDbClient,
    friend_public_key: PublicKey,
    identity_client: &mut IdentityClient,
    local_public_key: &PublicKey,
    max_operations_in_batch: usize,
    router_output: &mut RouterOutput,
) -> Result<(), RouterError> {
    match router_db_client
        .tc_db_client(friend_public_key.clone())
        .await?
        .ok_or(RouterError::InvalidState)?
        .get_tc_status()
        .await?
    {
        TcStatus::ConsistentIn(_) => {
            // Create an outgoing move token if we have something to send.
            let opt_tuple = handle_out_move_token_index_mutations_disallow_empty(
                router_db_client,
                identity_client,
                local_public_key,
                friend_public_key.clone(),
                max_operations_in_batch,
            )
            .await?;

            if let Some((move_token_request, index_mutations)) = opt_tuple {
                // We have something to send to remote side:

                // Update index mutations:
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
