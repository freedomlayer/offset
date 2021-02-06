use std::collections::{HashMap, HashSet};

use futures::StreamExt;

use derive_more::From;

use common::async_rpc::OpError;
use common::safe_arithmetic::{SafeSignedArithmetic, SafeUnsignedArithmetic};

use identity::IdentityClient;

use proto::app_server::messages::RelayAddressPort;
use proto::crypto::{NodePort, PublicKey};
use proto::funder::messages::{
    CancelSendFundsOp, Currency, FriendMessage, FriendTcOp, McBalance, MoveToken, MoveTokenRequest,
    RelaysUpdate, RequestSendFundsOp, ResponseSendFundsOp,
};
use proto::index_server::messages::{IndexMutation, RemoveFriendCurrency, UpdateFriendCurrency};
use proto::net::messages::NetAddress;

use crypto::rand::{CryptoRandom, RandGen};

use crate::route::Route;
use crate::router::types::{
    BackwardsOp, CurrencyInfo, RouterDbClient, RouterError, RouterOutput, RouterState, SentRelay,
};
use crate::token_channel::{TcDbClient, TcStatus, TokenChannelError};

/// Calculate send and receive capacity for a certain currency
/// This is the number we are going to report to an index server
pub fn calc_capacities(currency_info: &CurrencyInfo) -> Result<(u128, u128), RouterError> {
    // TODO:
    // Should also take into account:
    // - Liveness
    // - Open/Closed currencies
    // Maybe from the outside?
    todo!();

    if !currency_info.is_open {
        return Ok((0, 0));
    }

    let mc_balance = if let Some(mc_balance) = &currency_info.opt_mutual_credit {
        mc_balance.clone()
    } else {
        McBalance {
            balance: 0,
            local_pending_debt: 0,
            remote_pending_debt: 0,
            in_fees: 0.into(),
            out_fees: 0.into(),
        }
    };

    // local_max_debt + (balance - local_pending_debt)
    let send_capacity = currency_info.local_max_debt.saturating_add_signed(
        mc_balance
            .balance
            .checked_sub_unsigned(mc_balance.local_pending_debt)
            .ok_or(RouterError::InvalidState)?,
    );

    // remote_max_debt - (balance + remote_pending_debt)
    let recv_capacity = currency_info.remote_max_debt.saturating_sub_signed(
        mc_balance
            .balance
            .checked_add_unsigned(mc_balance.remote_pending_debt)
            .ok_or(RouterError::InvalidState)?,
    );

    Ok((send_capacity, recv_capacity))
}

/// Create one update index mutation, based on a given currency info.
pub fn create_update_index_mutation(
    friend_public_key: PublicKey,
    currency: Currency,
    currency_info: CurrencyInfo,
) -> Result<IndexMutation, RouterError> {
    let (send_capacity, recv_capacity) = calc_capacities(&currency_info)?;
    Ok(IndexMutation::UpdateFriendCurrency(UpdateFriendCurrency {
        public_key: friend_public_key,
        currency,
        send_capacity,
        recv_capacity,
        rate: currency_info.rate,
    }))
}

/*
/// Create a list of index mutations based on an outgoing MoveToken message.
/// Note that the outgoing MoveToken message was already applied.
pub async fn create_index_mutations_from_outgoing_move_token(
    router_db_client: &mut impl RouterDbClient,
    friend_public_key: PublicKey,
    move_token: &MoveToken,
) -> Result<Vec<IndexMutation>, RouterError> {
    // Strategy:
    // - For every currency in currencies diff:
    //      - If currency is gone, send a RemoveFriendCurrency
    //      - If currency exists, send UpdateFriendCurrency
    // - For the currencies from currencies operations, that were not in the currencies diff:
    //      - If currency is gone: Impossible
    //      - If currency exists: send UpdateFriendCurrency
    //
    // This strategy is consolidated into the following simplified strategy:
    // - For each mentioned currency (currencies_diff or currencies_operations):
    //      - If currency is gone, send a RemoveFriendCurrency
    //      - If currency exists, send UpdateFriendCurrency

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

    // TODO: Sort currencies set here, to get deterministic result.
    // Possibly use a function instead of inlining here.
    todo!();

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
                currency.clone(),
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
*/

/// Create a list of index mutations based on an incoming MoveToken message.
/// Note that the incoming MoveToken message was already applied.
pub async fn create_index_mutations_from_incoming_move_token(
    router_db_client: &mut impl RouterDbClient,
    friend_public_key: PublicKey,
    move_token: &MoveToken,
) -> Result<Vec<IndexMutation>, RouterError> {
    // Strategy:
    // - For every currency in currencies diff:
    //      - If currency is gone, send nothing (Because it was already gone before)
    //      - If currency exists, send UpdateFriendCurrency
    // - For the currencies from currencies operations, that were not in the currencies diff:
    //      - If currency is gone: Impossible
    //      - If currency exists: send UpdateFriendCurrency
    todo!();
}
