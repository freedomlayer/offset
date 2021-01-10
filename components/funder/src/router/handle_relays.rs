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

use crate::mutual_credit::incoming::IncomingMessage;
use crate::router::utils::index_mutation::create_update_index_mutation;
use crate::router::utils::move_token::{collect_outgoing_move_token, is_pending_move_token};
use crate::token_channel::{
    handle_in_move_token, ReceiveMoveTokenOutput, TcDbClient, TcStatus, TokenChannelError,
};

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
    // First we make sure that the friend exists:
    let _ = router_db_client
        .tc_db_client(friend_public_key.clone())
        .await?
        .ok_or(RouterError::InvalidDbState)?;

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
