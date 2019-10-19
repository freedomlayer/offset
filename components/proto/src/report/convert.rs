use std::collections::{HashMap, HashSet};
use std::fmt::Debug;

use common::mutable_state::MutableState;
use common::safe_arithmetic::{SafeSignedArithmetic, SafeUnsignedArithmetic};

use crate::crypto::PublicKey;

use crate::funder::messages::{Currency, Rate};
use crate::index_client::messages::{FriendInfo, IndexClientState};
use crate::index_server::messages::{IndexMutation, RemoveFriendCurrency, UpdateFriendCurrency};

use crate::report::messages::{
    ChannelStatusReport, FriendLivenessReport, FriendReport, FriendStatusReport, FunderReport,
    FunderReportMutation, RequestsStatusReport,
};

// Conversion to index client mutations and state
// ----------------------------------------------

// This code is used as glue between FunderReport structure and input mutations given to
// `index_client`. This allows the offst-index-client crate to not depend on the offst-funder
// crate.

// TODO: Maybe this logic shouldn't be here? Where should we move it to?
// TODO: Add tests (Mostly for arithmetic stuff here)

/// Calculate send and receive capacities for a given `friend_report`.
fn calc_friend_capacities<B>(friend_report: &FriendReport<B>) -> HashMap<Currency, (u128, u128)>
where
    B: Clone,
{
    if friend_report.status == FriendStatusReport::Disabled
        || friend_report.liveness == FriendLivenessReport::Offline
    {
        return HashMap::new();
        // return (0, 0);
    }

    let tc_report = match &friend_report.channel_status {
        ChannelStatusReport::Inconsistent(_) => return HashMap::new(),
        ChannelStatusReport::Consistent(channel_consistent_report) => {
            &channel_consistent_report.tc_report
        }
    };

    tc_report
        .currency_reports
        .iter()
        .map(|currency_report| {
            let balance = &currency_report.balance;
            let send_capacity =
                if currency_report.requests_status.remote == RequestsStatusReport::Closed {
                    0
                } else {
                    // local_max_debt + balance - local_pending_debt
                    balance.local_max_debt.saturating_add_signed(
                        balance
                            .balance
                            .checked_sub_unsigned(balance.local_pending_debt)
                            .unwrap(),
                    )
                };

            let recv_capacity =
                if currency_report.requests_status.local == RequestsStatusReport::Closed {
                    0
                } else {
                    balance.remote_max_debt.saturating_sub_signed(
                        balance
                            .balance
                            .checked_add_unsigned(balance.remote_pending_debt)
                            .unwrap(),
                    )
                };

            (
                currency_report.currency.clone(),
                (send_capacity, recv_capacity),
            )
        })
        .collect()
}

fn calc_friends_info<B>(
    funder_report: &FunderReport<B>,
) -> impl Iterator<Item = ((PublicKey, Currency), FriendInfo)> + '_
where
    B: Clone,
{
    funder_report
        .friends
        .iter()
        .flat_map(|(friend_public_key, friend_report)| {
            calc_friend_capacities(friend_report).into_iter().map(
                move |(currency, (send_capacity, recv_capacity))| {
                    let rate = friend_report
                        .rates
                        .iter()
                        .find(|currency_rate| currency_rate.currency == currency)
                        .map(|currency_rate| currency_rate.rate.clone())
                        .unwrap_or(Rate::new());

                    (
                        (friend_public_key.clone(), currency),
                        FriendInfo {
                            send_capacity,
                            recv_capacity,
                            rate,
                        },
                    )
                },
            )
        })
        .filter(|(_, friend_info)| friend_info.send_capacity != 0 || friend_info.recv_capacity != 0)
}

pub fn funder_report_to_index_client_state<B>(funder_report: &FunderReport<B>) -> IndexClientState
where
    B: Clone,
{
    IndexClientState {
        friends: calc_friends_info(funder_report).collect(),
    }
}

fn calc_index_mutations<B>(
    old_funder_report: &FunderReport<B>,
    new_funder_report: &FunderReport<B>,
) -> Vec<IndexMutation>
where
    B: Clone,
{
    let old_friends_info: HashMap<(PublicKey, Currency), FriendInfo> =
        calc_friends_info(old_funder_report).collect();
    let new_friends_info: HashMap<(PublicKey, Currency), FriendInfo> =
        calc_friends_info(new_funder_report).collect();

    let old_keys: HashSet<(PublicKey, Currency)> = old_friends_info.keys().cloned().collect();
    let new_keys: HashSet<(PublicKey, Currency)> = new_friends_info.keys().cloned().collect();

    let mut res_mutations = Vec::new();

    // Push Remove mutations:
    for (public_key, currency) in old_keys.difference(&new_keys) {
        res_mutations.push(IndexMutation::RemoveFriendCurrency(RemoveFriendCurrency {
            public_key: public_key.clone(),
            currency: currency.clone(),
        }));
    }

    // Push update mutations:
    for ((public_key, currency), friend_info) in new_friends_info {
        res_mutations.push(IndexMutation::UpdateFriendCurrency(UpdateFriendCurrency {
            public_key,
            currency,
            send_capacity: friend_info.send_capacity,
            recv_capacity: friend_info.recv_capacity,
            rate: friend_info.rate,
        }));
    }
    res_mutations
}

pub fn funder_report_mutation_to_index_mutation<B>(
    funder_report: &FunderReport<B>,
    funder_report_mutation: &FunderReportMutation<B>,
) -> Vec<IndexMutation>
where
    B: Clone + Debug,
{
    let mut new_funder_report = funder_report.clone();
    new_funder_report.mutate(funder_report_mutation).unwrap();

    calc_index_mutations(funder_report, &new_funder_report)
}

// TODO: Add tests.
