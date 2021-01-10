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
use crate::router::utils::index_mutation::create_update_index_mutation;
use crate::router::utils::move_token::{
    handle_out_move_token_index_mutations, is_pending_move_token,
};
use crate::token_channel::{
    handle_in_move_token, ReceiveMoveTokenOutput, TcDbClient, TcStatus, TokenChannelError,
};

pub async fn add_currency(
    router_db_client: &mut impl RouterDbClient,
    friend_public_key: PublicKey,
    currency: Currency,
    identity_client: &mut IdentityClient,
    local_public_key: &PublicKey,
    max_operations_in_batch: usize,
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
    // First we make sure that the friend exists:
    let mut output = RouterOutput::new();
    if router_db_client
        .tc_db_client(friend_public_key.clone())
        .await?
        .is_none()
    {
        return Ok(output);
    }
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
    // First we make sure that the friend exists:
    let mut output = RouterOutput::new();
    if router_db_client
        .tc_db_client(friend_public_key.clone())
        .await?
        .is_none()
    {
        return Ok(output);
    }
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
    // First we make sure that the friend exists:
    let mut output = RouterOutput::new();
    if router_db_client
        .tc_db_client(friend_public_key.clone())
        .await?
        .is_none()
    {
        return Ok(output);
    }

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
    // First we make sure that the friend exists:
    let mut output = RouterOutput::new();
    if router_db_client
        .tc_db_client(friend_public_key.clone())
        .await?
        .is_none()
    {
        return Ok(output);
    }

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
    // First we make sure that the friend exists:
    let mut output = RouterOutput::new();
    if router_db_client
        .tc_db_client(friend_public_key.clone())
        .await?
        .is_none()
    {
        return Ok(output);
    }

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
