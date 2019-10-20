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
    for (public_key_currency, friend_info) in new_friends_info {
        if let Some(old_friend_info) = old_friends_info.get(&public_key_currency) {
            if old_friend_info == &friend_info {
                continue;
            }
        }
        let (public_key, currency) = public_key_currency;
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

#[cfg(test)]
mod tests {
    use super::*;

    use crate::report::messages::{
        ChannelConsistentReport, CurrencyRate, CurrencyReport, DirectionReport, McBalanceReport,
        McRequestsStatusReport, RequestsStatusReport, SentLocalRelaysReport, TcReport,
    };
    use std::convert::TryFrom;

    #[test]
    fn test_calc_friends_info() {
        let currency1 = Currency::try_from("FST1".to_owned()).unwrap();
        let currency2 = Currency::try_from("FST2".to_owned()).unwrap();
        let currency3 = Currency::try_from("FST3".to_owned()).unwrap();

        let pk1 = PublicKey::from(&[1; PublicKey::len()]);
        let pk2 = PublicKey::from(&[2; PublicKey::len()]);
        let pk3 = PublicKey::from(&[3; PublicKey::len()]);

        let mut friends = HashMap::new();
        friends.insert(
            pk2.clone(),
            FriendReport::<u32> {
                name: "friend_name".to_owned(),
                rates: vec![CurrencyRate {
                    currency: currency3.clone(),
                    rate: Rate { mul: 1, add: 10 },
                }],
                remote_relays: vec![],
                sent_local_relays: SentLocalRelaysReport::NeverSent,
                opt_last_incoming_move_token: None,
                liveness: FriendLivenessReport::Online,
                channel_status: ChannelStatusReport::Consistent(ChannelConsistentReport {
                    tc_report: TcReport {
                        direction: DirectionReport::Incoming,
                        currency_reports: vec![
                            CurrencyReport {
                                currency: currency1.clone(),
                                balance: McBalanceReport {
                                    balance: 0,
                                    local_max_debt: 100,
                                    remote_max_debt: 200,
                                    local_pending_debt: 0,
                                    remote_pending_debt: 0,
                                },
                                requests_status: McRequestsStatusReport {
                                    local: RequestsStatusReport::Open,
                                    remote: RequestsStatusReport::Open,
                                },
                            },
                            CurrencyReport {
                                currency: currency2.clone(),
                                balance: McBalanceReport {
                                    balance: 0,
                                    local_max_debt: 100,
                                    remote_max_debt: 200,
                                    local_pending_debt: 0,
                                    remote_pending_debt: 0,
                                },
                                requests_status: McRequestsStatusReport {
                                    local: RequestsStatusReport::Closed,
                                    remote: RequestsStatusReport::Open,
                                },
                            },
                            CurrencyReport {
                                currency: currency3.clone(),
                                balance: McBalanceReport {
                                    balance: 50,
                                    local_max_debt: 100,
                                    remote_max_debt: 200,
                                    local_pending_debt: 10,
                                    remote_pending_debt: 30,
                                },
                                requests_status: McRequestsStatusReport {
                                    local: RequestsStatusReport::Open,
                                    remote: RequestsStatusReport::Open,
                                },
                            },
                        ],
                    },
                }),
                status: FriendStatusReport::Enabled,
            },
        );

        friends.insert(
            pk3.clone(),
            FriendReport::<u32> {
                name: "friend_name".to_owned(),
                rates: vec![CurrencyRate {
                    currency: currency1.clone(),
                    rate: Rate { mul: 2, add: 2 },
                }],
                remote_relays: vec![],
                sent_local_relays: SentLocalRelaysReport::NeverSent,
                opt_last_incoming_move_token: None,
                liveness: FriendLivenessReport::Online,
                channel_status: ChannelStatusReport::Consistent(ChannelConsistentReport {
                    tc_report: TcReport {
                        direction: DirectionReport::Incoming,
                        currency_reports: vec![CurrencyReport {
                            currency: currency1.clone(),
                            balance: McBalanceReport {
                                balance: 0,
                                local_max_debt: 100,
                                remote_max_debt: 200,
                                local_pending_debt: 0,
                                remote_pending_debt: 0,
                            },
                            requests_status: McRequestsStatusReport {
                                local: RequestsStatusReport::Open,
                                remote: RequestsStatusReport::Open,
                            },
                        }],
                    },
                }),
                status: FriendStatusReport::Enabled,
            },
        );
        let funder_report = FunderReport {
            local_public_key: pk1.clone(),
            relays: Vec::new(),
            friends,
        };
        let friends_info: HashMap<(PublicKey, Currency), FriendInfo> =
            calc_friends_info(&funder_report).collect();

        let friend_info = friends_info.get(&(pk2.clone(), currency1.clone())).unwrap();
        assert_eq!(friend_info.send_capacity, 100);
        assert_eq!(friend_info.recv_capacity, 200);
        assert_eq!(friend_info.rate, Rate::new());

        let friend_info = friends_info.get(&(pk2.clone(), currency2.clone())).unwrap();
        assert_eq!(friend_info.send_capacity, 100);
        assert_eq!(friend_info.recv_capacity, 0);
        assert_eq!(friend_info.rate, Rate::new());

        let friend_info = friends_info.get(&(pk2.clone(), currency3.clone())).unwrap();
        assert_eq!(friend_info.send_capacity, 140);
        assert_eq!(friend_info.recv_capacity, 120);
        assert_eq!(friend_info.rate, Rate { mul: 1, add: 10 });

        let friend_info = friends_info.get(&(pk3.clone(), currency1.clone())).unwrap();
        assert_eq!(friend_info.send_capacity, 100);
        assert_eq!(friend_info.recv_capacity, 200);
        assert_eq!(friend_info.rate, Rate { mul: 2, add: 2 });
    }

    #[test]
    fn test_calc_index_mutations() {
        let currency1 = Currency::try_from("FST1".to_owned()).unwrap();
        let currency2 = Currency::try_from("FST2".to_owned()).unwrap();
        let currency3 = Currency::try_from("FST3".to_owned()).unwrap();
        let currency4 = Currency::try_from("FST4".to_owned()).unwrap();

        let pk1 = PublicKey::from(&[1; PublicKey::len()]);
        let pk2 = PublicKey::from(&[2; PublicKey::len()]);
        let _pk3 = PublicKey::from(&[3; PublicKey::len()]);

        let mut friends = HashMap::new();
        friends.insert(
            pk2.clone(),
            FriendReport::<u32> {
                name: "friend_name".to_owned(),
                rates: vec![],
                remote_relays: vec![],
                sent_local_relays: SentLocalRelaysReport::NeverSent,
                opt_last_incoming_move_token: None,
                liveness: FriendLivenessReport::Online,
                channel_status: ChannelStatusReport::Consistent(ChannelConsistentReport {
                    tc_report: TcReport {
                        direction: DirectionReport::Incoming,
                        currency_reports: vec![
                            CurrencyReport {
                                currency: currency1.clone(),
                                balance: McBalanceReport {
                                    balance: 0,
                                    local_max_debt: 100,
                                    remote_max_debt: 200,
                                    local_pending_debt: 0,
                                    remote_pending_debt: 0,
                                },
                                requests_status: McRequestsStatusReport {
                                    local: RequestsStatusReport::Open,
                                    remote: RequestsStatusReport::Open,
                                },
                            },
                            CurrencyReport {
                                currency: currency2.clone(),
                                balance: McBalanceReport {
                                    balance: 0,
                                    local_max_debt: 100,
                                    remote_max_debt: 200,
                                    local_pending_debt: 0,
                                    remote_pending_debt: 0,
                                },
                                requests_status: McRequestsStatusReport {
                                    local: RequestsStatusReport::Closed,
                                    remote: RequestsStatusReport::Open,
                                },
                            },
                            CurrencyReport {
                                currency: currency4.clone(),
                                balance: McBalanceReport {
                                    balance: 0,
                                    local_max_debt: 30,
                                    remote_max_debt: 40,
                                    local_pending_debt: 0,
                                    remote_pending_debt: 0,
                                },
                                requests_status: McRequestsStatusReport {
                                    local: RequestsStatusReport::Open,
                                    remote: RequestsStatusReport::Open,
                                },
                            },
                        ],
                    },
                }),
                status: FriendStatusReport::Enabled,
            },
        );

        let old_funder_report = FunderReport {
            local_public_key: pk1.clone(),
            relays: Vec::new(),
            friends,
        };

        let mut friends = HashMap::new();
        friends.insert(
            pk2.clone(),
            FriendReport::<u32> {
                name: "friend_name".to_owned(),
                rates: vec![CurrencyRate {
                    currency: currency3.clone(),
                    rate: Rate { mul: 1, add: 10 },
                }],
                remote_relays: vec![],
                sent_local_relays: SentLocalRelaysReport::NeverSent,
                opt_last_incoming_move_token: None,
                liveness: FriendLivenessReport::Online,
                channel_status: ChannelStatusReport::Consistent(ChannelConsistentReport {
                    tc_report: TcReport {
                        direction: DirectionReport::Incoming,
                        currency_reports: vec![
                            CurrencyReport {
                                currency: currency1.clone(),
                                balance: McBalanceReport {
                                    balance: 0,
                                    local_max_debt: 50,
                                    remote_max_debt: 300,
                                    local_pending_debt: 0,
                                    remote_pending_debt: 0,
                                },
                                requests_status: McRequestsStatusReport {
                                    local: RequestsStatusReport::Open,
                                    remote: RequestsStatusReport::Open,
                                },
                            },
                            CurrencyReport {
                                currency: currency3.clone(),
                                balance: McBalanceReport {
                                    balance: 50,
                                    local_max_debt: 100,
                                    remote_max_debt: 200,
                                    local_pending_debt: 10,
                                    remote_pending_debt: 30,
                                },
                                requests_status: McRequestsStatusReport {
                                    local: RequestsStatusReport::Open,
                                    remote: RequestsStatusReport::Open,
                                },
                            },
                            CurrencyReport {
                                currency: currency4.clone(),
                                balance: McBalanceReport {
                                    balance: 0,
                                    local_max_debt: 30,
                                    remote_max_debt: 40,
                                    local_pending_debt: 0,
                                    remote_pending_debt: 0,
                                },
                                requests_status: McRequestsStatusReport {
                                    local: RequestsStatusReport::Open,
                                    remote: RequestsStatusReport::Open,
                                },
                            },
                        ],
                    },
                }),
                status: FriendStatusReport::Enabled,
            },
        );
        let new_funder_report = FunderReport {
            local_public_key: pk1.clone(),
            relays: Vec::new(),
            friends,
        };

        let index_mutations = calc_index_mutations(&old_funder_report, &new_funder_report);
        assert_eq!(index_mutations.len(), 3);
        for index_mutation in index_mutations {
            match index_mutation {
                IndexMutation::RemoveFriendCurrency(remove_friend_currency) => {
                    assert_eq!(remove_friend_currency.public_key, pk2);
                    assert_eq!(remove_friend_currency.currency, currency2);
                }
                IndexMutation::UpdateFriendCurrency(update_friend_currency) => {
                    if update_friend_currency.currency == currency3 {
                        assert_eq!(update_friend_currency.public_key, pk2);
                        assert_eq!(update_friend_currency.send_capacity, 140);
                        assert_eq!(update_friend_currency.recv_capacity, 120);
                        assert_eq!(update_friend_currency.rate, Rate { mul: 1, add: 10 });
                    } else {
                        assert_eq!(update_friend_currency.public_key, pk2);
                        assert_eq!(update_friend_currency.send_capacity, 50);
                        assert_eq!(update_friend_currency.recv_capacity, 300);
                        assert_eq!(update_friend_currency.rate, Rate::new());
                    }
                }
            }
        }
    }
}
