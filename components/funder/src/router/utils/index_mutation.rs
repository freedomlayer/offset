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
fn calc_capacities(currency_info: &CurrencyInfo) -> Result<(u128, u128), RouterError> {
    // TODO: Should also take into account liveness?  Maybe from the outside?

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

/// Create one index mutation, based on a given currency info.
/// Returns IndexMutation::UpdateFriendCurrency if there is any send/recv capacity.
/// Otherwise, returns IndexMutation::RemoveFriendCurrency
pub fn create_index_mutation(
    friend_public_key: PublicKey,
    currency: Currency,
    currency_info: CurrencyInfo,
) -> Result<IndexMutation, RouterError> {
    let (send_capacity, recv_capacity) = calc_capacities(&currency_info)?;
    Ok(if send_capacity == 0 && recv_capacity == 0 {
        IndexMutation::RemoveFriendCurrency(RemoveFriendCurrency {
            public_key: friend_public_key,
            currency,
        })
    } else {
        IndexMutation::UpdateFriendCurrency(UpdateFriendCurrency {
            public_key: friend_public_key,
            currency,
            send_capacity,
            recv_capacity,
            rate: currency_info.rate,
        })
    })
}
