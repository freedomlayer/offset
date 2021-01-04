use std::collections::{HashMap, HashSet};

use futures::StreamExt;

use derive_more::From;

use common::async_rpc::OpError;
use common::safe_arithmetic::{SafeSignedArithmetic, SafeUnsignedArithmetic};

use identity::IdentityClient;

use proto::app_server::messages::RelayAddressPort;
use proto::crypto::{NodePort, PublicKey};
use proto::funder::messages::{
    CancelSendFundsOp, CurrenciesOperations, Currency, CurrencyOperations, FriendMessage,
    FriendTcOp, MoveToken, MoveTokenRequest, RelaysUpdate, RequestSendFundsOp, ResponseSendFundsOp,
};
use proto::index_server::messages::{IndexMutation, RemoveFriendCurrency, UpdateFriendCurrency};
use proto::net::messages::NetAddress;

use crypto::rand::{CryptoRandom, RandGen};

use crate::route::Route;
use crate::router::types::{
    BackwardsOp, CurrencyInfo, RouterDbClient, RouterError, RouterOutput, RouterState, SentRelay,
};
use crate::token_channel::{handle_out_move_token, TcDbClient, TcStatus, TokenChannelError};

/// Calculate receive capacity for a certain currency
/// This is the number we are going to report to an index server
fn calc_recv_capacity(currency_info: &CurrencyInfo) -> Result<u128, RouterError> {
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
            .ok_or(RouterError::BalanceOverflow)?,
    ))
}

/// Create one update index mutation, based on a given currency info.
pub fn create_update_index_mutation(
    friend_public_key: PublicKey,
    currency_info: CurrencyInfo,
) -> Result<IndexMutation, RouterError> {
    let recv_capacity = calc_recv_capacity(&currency_info)?;
    Ok(IndexMutation::UpdateFriendCurrency(UpdateFriendCurrency {
        public_key: friend_public_key,
        currency: currency_info.currency,
        recv_capacity,
        rate: currency_info.rate,
    }))
}

/// Create a list of index mutations based on a MoveToken message.
pub async fn create_index_mutations_from_move_token(
    router_db_client: &mut impl RouterDbClient,
    friend_public_key: PublicKey,
    move_token: &MoveToken,
) -> Result<Vec<IndexMutation>, RouterError> {
    // Collect all mentioned currencies:
    let currencies = {
        let mut currencies = HashSet::new();
        for currency in &move_token.currencies_diff {
            currencies.insert(currency.clone());
        }

        for (currency, _operation) in &move_token.currencies_operations {
            currencies.insert(currency.clone());
        }
        currencies
    };

    let mut index_mutations = Vec::new();

    // Create all index mutations:
    for currency in currencies.into_iter() {
        let opt_currency_info = router_db_client
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
