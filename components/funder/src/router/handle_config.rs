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
use crate::router::utils::flush::flush_friend;
use crate::router::utils::index_mutation::create_index_mutation;
use crate::router::utils::move_token::is_pending_move_token;
use crate::token_channel::{
    handle_in_move_token, ReceiveMoveTokenOutput, TcDbClient, TcStatus, TokenChannelError,
};

pub async fn add_currency(
    control: &mut impl RouterControl,
    info: &RouterInfo,
    friend_public_key: PublicKey,
    currency: Currency,
) -> Result<(), RouterError> {
    let access = control.access();
    // First we make sure that the friend exists:
    if access
        .router_db_client
        .tc_db_client(friend_public_key.clone())
        .await?
        .is_none()
    {
        return Ok(());
    }
    access
        .router_db_client
        .add_currency_config(friend_public_key.clone(), currency)
        .await?;

    access.send_commands.insert(friend_public_key);

    Ok(())
}

pub async fn set_remove_currency(
    control: &mut impl RouterControl,
    info: &RouterInfo,
    friend_public_key: PublicKey,
    currency: Currency,
) -> Result<(), RouterError> {
    // Revise implementation with respect to new design
    // First we make sure that the friend exists:
    if control
        .access()
        .router_db_client
        .tc_db_client(friend_public_key.clone())
        .await?
        .is_none()
    {
        return Ok(());
    }
    control
        .access()
        .router_db_client
        .set_currency_remove(friend_public_key.clone(), currency)
        .await?;

    control.access().send_commands.insert(friend_public_key);
    Ok(())
}

pub async fn unset_remove_currency(
    control: &mut impl RouterControl,
    info: &RouterInfo,
    friend_public_key: PublicKey,
    currency: Currency,
) -> Result<(), RouterError> {
    // First we make sure that the friend exists:
    if control
        .access()
        .router_db_client
        .tc_db_client(friend_public_key.clone())
        .await?
        .is_none()
    {
        return Ok(());
    }
    control
        .access()
        .router_db_client
        .unset_currency_remove(friend_public_key.clone(), currency)
        .await?;

    control.access().send_commands.insert(friend_public_key);
    Ok(())
}

pub async fn set_remote_max_debt(
    control: &mut impl RouterControl,
    info: &RouterInfo,
    friend_public_key: PublicKey,
    currency: Currency,
    remote_max_debt: u128,
) -> Result<(), RouterError> {
    let access = control.access();
    // First we make sure that the friend exists:
    if access
        .router_db_client
        .tc_db_client(friend_public_key.clone())
        .await?
        .is_none()
    {
        return Ok(());
    }

    let opt_currency_info = access
        .router_db_client
        .get_currency_info(friend_public_key.clone(), currency.clone())
        .await?;
    if let Some(currency_info) = opt_currency_info {
        // Currency exists (We don't do anything otherwise)
        // Set remote max debt:
        access
            .router_db_client
            .set_remote_max_debt(friend_public_key.clone(), currency.clone(), remote_max_debt)
            .await?;

        // TODO: It is possible that the capacity was already zero, and when we changed
        // remote_max_debt the capacity has somehow stayed zero. In that case we will not need to
        // resend an index mutation. Currently we do send. It wastes bandwidth, but it is still
        // correct. Maybe in the future we can decide more elegantly when to send an index
        // mutation.

        // Create an index mutation if needed:
        if currency_info.is_open {
            // Currency is open:
            access.output.add_index_mutation(create_index_mutation(
                friend_public_key.clone(),
                currency,
                currency_info,
            )?);
        }
    }

    Ok(())
}

pub async fn set_local_max_debt(
    control: &mut impl RouterControl,
    info: &RouterInfo,
    friend_public_key: PublicKey,
    currency: Currency,
    local_max_debt: u128,
) -> Result<(), RouterError> {
    let access = control.access();
    // First we make sure that the friend exists:
    if access
        .router_db_client
        .tc_db_client(friend_public_key.clone())
        .await?
        .is_none()
    {
        return Ok(());
    }

    let opt_currency_info = access
        .router_db_client
        .get_currency_info(friend_public_key.clone(), currency.clone())
        .await?;
    if let Some(currency_info) = opt_currency_info {
        // Currency exists (We don't do anything otherwise)
        // Set remote max debt:
        access
            .router_db_client
            .set_local_max_debt(friend_public_key.clone(), currency.clone(), local_max_debt)
            .await?;

        // TODO: It is possible that the capacity was already zero, and when we changed
        // remote_max_debt the capacity has somehow stayed zero. In that case we will not need to
        // resend an index mutation. Currently we do send. It wastes bandwidth, but it is still
        // correct. Maybe in the future we can decide more elegantly when to send an index
        // mutation.

        // Create an index mutation if needed:
        if currency_info.is_open {
            // Currency is open:
            access.output.add_index_mutation(create_index_mutation(
                friend_public_key.clone(),
                currency,
                currency_info,
            )?);
        }
    }

    Ok(())
}

pub async fn open_currency(
    control: &mut impl RouterControl,
    friend_public_key: PublicKey,
    currency: Currency,
) -> Result<(), RouterError> {
    let access = control.access();
    // First we make sure that the friend exists:
    if access
        .router_db_client
        .tc_db_client(friend_public_key.clone())
        .await?
        .is_none()
    {
        return Ok(());
    }

    let opt_currency_info = access
        .router_db_client
        .get_currency_info(friend_public_key.clone(), currency.clone())
        .await?;

    if let Some(currency_info) = opt_currency_info {
        // Currency exists:
        if !currency_info.is_open {
            // currency is closed:

            // Open currency:
            access
                .router_db_client
                .open_currency(friend_public_key.clone(), currency.clone())
                .await?;

            let index_mutation = create_index_mutation(friend_public_key, currency, currency_info)?;
            if matches!(index_mutation, IndexMutation::UpdateFriendCurrency(..)) {
                // Add index mutation:
                access.output.add_index_mutation(index_mutation);
            }
        }
    }

    Ok(())
}

pub async fn close_currency(
    control: &mut impl RouterControl,
    friend_public_key: PublicKey,
    currency: Currency,
) -> Result<(), RouterError> {
    let access = control.access();
    // First we make sure that the friend exists:
    if access
        .router_db_client
        .tc_db_client(friend_public_key.clone())
        .await?
        .is_none()
    {
        return Ok(());
    }

    let opt_currency_info = access
        .router_db_client
        .get_currency_info(friend_public_key.clone(), currency.clone())
        .await?;

    if let Some(currency_info) = opt_currency_info {
        // Currency exists:
        if currency_info.is_open {
            // currency is open:

            // Close currency:
            access
                .router_db_client
                .close_currency(friend_public_key.clone(), currency.clone())
                .await?;

            // Add index mutation:
            access
                .output
                .add_index_mutation(IndexMutation::RemoveFriendCurrency(RemoveFriendCurrency {
                    public_key: friend_public_key.clone(),
                    currency,
                }));

            // Cancel all user requests pending for this currency:
            while let Some(mc_request) = access
                .router_db_client
                .pending_user_requests_pop_front_by_currency(friend_public_key.clone())
                .await?
            {
                todo!();
            }

            // Cancel all requests pending for this currency:
            while let Some(mc_request) = access
                .router_db_client
                .pending_user_requests_pop_front_by_currency(friend_public_key.clone())
                .await?
            {
                todo!();
            }
        }
    }

    Ok(())
}
