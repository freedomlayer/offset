use im::hashmap::HashMap as ImHashMap;

use common::int_convert::usize_to_u64;
use signature::canonical::CanonicalSerialize;

use proto::report::messages::{
    AddFriendReport, ChannelConsistentReport, ChannelInconsistentReport, ChannelStatusReport,
    CurrencyReport, DirectionReport, FriendLivenessReport, FriendReport, FriendReportMutation,
    FriendStatusReport, FunderReport, FunderReportMutation, McBalanceReport,
    McRequestsStatusReport, MoveTokenHashedReport, RelaysTransitionReport, RequestsStatusReport,
    ResetTermsReport, SentLocalRelaysReport, TcReport,
};

use crate::types::MoveTokenHashed;

use crate::ephemeral::{Ephemeral, EphemeralMutation};
use crate::friend::{ChannelStatus, FriendMutation, FriendState, SentLocalRelays};
use crate::liveness::LivenessMutation;
use crate::mutual_credit::types::{McBalance, McRequestsStatus};
use crate::state::{FunderMutation, FunderState};
use crate::token_channel::TokenChannel;

impl<B> Into<SentLocalRelaysReport<B>> for &SentLocalRelays<B>
where
    B: Clone,
{
    fn into(self) -> SentLocalRelaysReport<B> {
        match self {
            SentLocalRelays::NeverSent => SentLocalRelaysReport::NeverSent,
            SentLocalRelays::Transition((last_sent, before_last_sent)) => {
                SentLocalRelaysReport::Transition(RelaysTransitionReport {
                    last_sent: last_sent.into_iter().cloned().collect(),
                    before_last_sent: before_last_sent.into_iter().cloned().collect(),
                })
            }
            SentLocalRelays::LastSent(address) => {
                SentLocalRelaysReport::LastSent(address.clone().into_iter().collect())
            }
        }
    }
}

impl From<&McRequestsStatus> for McRequestsStatusReport {
    fn from(mc_requests_status: &McRequestsStatus) -> McRequestsStatusReport {
        McRequestsStatusReport {
            local: (&mc_requests_status.local).into(),
            remote: (&mc_requests_status.remote).into(),
        }
    }
}

impl From<&McBalance> for McBalanceReport {
    fn from(mc_balance: &McBalance) -> McBalanceReport {
        McBalanceReport {
            balance: mc_balance.balance,
            remote_max_debt: mc_balance.remote_max_debt,
            local_max_debt: mc_balance.local_max_debt,
            local_pending_debt: mc_balance.local_pending_debt,
            remote_pending_debt: mc_balance.remote_pending_debt,
        }
    }
}

impl From<&MoveTokenHashed> for MoveTokenHashedReport {
    fn from(move_token_hashed: &MoveTokenHashed) -> MoveTokenHashedReport {
        MoveTokenHashedReport {
            prefix_hash: move_token_hashed.prefix_hash.clone(),
            token_info: move_token_hashed.token_info.clone(),
            rand_nonce: move_token_hashed.rand_nonce.clone(),
            new_token: move_token_hashed.new_token.clone(),
        }
    }
}

impl<B> From<&TokenChannel<B>> for TcReport
where
    B: Clone + CanonicalSerialize,
{
    fn from(token_channel: &TokenChannel<B>) -> TcReport {
        let currency_reports = token_channel
            .get_mutual_credits()
            .iter()
            .map(|(currency, mutual_credit)| {
                let mc_state = mutual_credit.state();
                CurrencyReport {
                    currency: currency.clone(),
                    balance: McBalanceReport::from(&mc_state.balance),
                    requests_status: McRequestsStatusReport::from(&mc_state.requests_status),
                    num_local_pending_requests: usize_to_u64(
                        mc_state.pending_transactions.local.len(),
                    )
                    .unwrap(),
                    num_remote_pending_requests: usize_to_u64(
                        mc_state.pending_transactions.remote.len(),
                    )
                    .unwrap(),
                }
            })
            .collect();

        let direction = if token_channel.get_outgoing().is_some() {
            DirectionReport::Outgoing
        } else {
            DirectionReport::Incoming
        };

        TcReport {
            direction,
            currency_reports,
        }
    }
}

impl<B> From<&ChannelStatus<B>> for ChannelStatusReport
where
    B: Clone + CanonicalSerialize,
{
    fn from(channel_status: &ChannelStatus<B>) -> ChannelStatusReport {
        match channel_status {
            ChannelStatus::Inconsistent(channel_inconsistent) => {
                let opt_remote_reset_terms = channel_inconsistent
                    .opt_remote_reset_terms
                    .clone()
                    .map(|remote_reset_terms| ResetTermsReport {
                        reset_token: remote_reset_terms.reset_token.clone(),
                        balance_for_reset: remote_reset_terms.balance_for_reset,
                    });
                let channel_inconsistent_report = ChannelInconsistentReport {
                    local_reset_terms: channel_inconsistent
                        .local_reset_terms
                        .balance_for_reset
                        .clone(),
                    opt_remote_reset_terms,
                };
                ChannelStatusReport::Inconsistent(channel_inconsistent_report)
            }
            ChannelStatus::Consistent(channel_consistent) => {
                let channel_consistent_report = ChannelConsistentReport {
                    tc_report: TcReport::from(&channel_consistent.token_channel),
                    num_pending_requests: usize_to_u64(channel_consistent.pending_requests.len())
                        .unwrap(),
                    num_pending_backwards_ops: usize_to_u64(
                        channel_consistent.pending_backwards_ops.len(),
                    )
                    .unwrap(),
                    num_pending_user_requests: usize_to_u64(
                        channel_consistent.pending_user_requests.len(),
                    )
                    .unwrap(),
                };
                ChannelStatusReport::Consistent(channel_consistent_report)
            }
        }
    }
}

fn create_friend_report<B>(
    friend_state: &FriendState<B>,
    friend_liveness: &FriendLivenessReport,
) -> FriendReport<B>
where
    B: Clone + CanonicalSerialize,
{
    let channel_status = ChannelStatusReport::from(&friend_state.channel_status);

    FriendReport {
        name: friend_state.name.clone(),
        rate: friend_state.rate.clone(),
        remote_relays: friend_state.remote_relays.clone(),
        sent_local_relays: (&friend_state.sent_local_relays).into(),
        opt_last_incoming_move_token: friend_state
            .channel_status
            .get_last_incoming_move_token_hashed()
            .map(|move_token_hashed| MoveTokenHashedReport::from(&move_token_hashed)),
        liveness: friend_liveness.clone(),
        channel_status,
        wanted_remote_max_debt: friend_state.wanted_remote_max_debt,
        wanted_local_requests_status: RequestsStatusReport::from(
            &friend_state.wanted_local_requests_status,
        ),
        status: FriendStatusReport::from(&friend_state.status),
    }
}

pub fn create_report<B>(funder_state: &FunderState<B>, ephemeral: &Ephemeral) -> FunderReport<B>
where
    B: Clone + CanonicalSerialize,
{
    let mut friends = ImHashMap::new();
    for (friend_public_key, friend_state) in &funder_state.friends {
        let friend_liveness = if ephemeral.liveness.is_online(friend_public_key) {
            FriendLivenessReport::Online
        } else {
            FriendLivenessReport::Offline
        };
        let friend_report = create_friend_report(&friend_state, &friend_liveness);
        friends.insert(friend_public_key.clone(), friend_report);
    }

    FunderReport {
        local_public_key: funder_state.local_public_key.clone(),
        relays: funder_state.relays.clone().into_iter().collect(),
        friends: friends.clone().into_iter().collect(),
        num_open_invoices: usize_to_u64(funder_state.open_invoices.len()).unwrap(),
        num_payments: usize_to_u64(funder_state.payments.len()).unwrap(),
        num_open_transactions: usize_to_u64(funder_state.open_transactions.len()).unwrap(),
    }
}

pub fn create_initial_report<B>(funder_state: &FunderState<B>) -> FunderReport<B>
where
    B: Clone + CanonicalSerialize,
{
    create_report(funder_state, &Ephemeral::new())
}

pub fn friend_mutation_to_report_mutations<B>(
    friend_mutation: &FriendMutation<B>,
    friend: &FriendState<B>,
) -> Vec<FriendReportMutation<B>>
where
    B: Clone + CanonicalSerialize,
{
    let mut friend_after = friend.clone();
    friend_after.mutate(friend_mutation);
    match friend_mutation {
        FriendMutation::TcMutation(_tc_mutation) => {
            let channel_status_report = ChannelStatusReport::from(&friend_after.channel_status);
            let set_channel_status = FriendReportMutation::SetChannelStatus(channel_status_report);
            let set_last_incoming_move_token = FriendReportMutation::SetOptLastIncomingMoveToken(
                friend_after
                    .channel_status
                    .get_last_incoming_move_token_hashed()
                    .map(|move_token_hashed| MoveTokenHashedReport::from(&move_token_hashed)),
            );
            vec![set_channel_status, set_last_incoming_move_token]
        }
        FriendMutation::SetWantedRemoteMaxDebt(wanted_remote_max_debt) => {
            vec![FriendReportMutation::SetWantedRemoteMaxDebt(
                *wanted_remote_max_debt,
            )]
        }
        FriendMutation::SetWantedLocalRequestsStatus(requests_status) => {
            vec![FriendReportMutation::SetWantedLocalRequestsStatus(
                RequestsStatusReport::from(requests_status),
            )]
        }
        FriendMutation::PushBackPendingRequest(_request_send_funds) => {
            let channel_consistent = if let ChannelStatus::Consistent(channel_consistent) =
                &friend_after.channel_status
            {
                channel_consistent
            } else {
                unreachable!();
            };
            vec![FriendReportMutation::SetNumPendingRequests(
                usize_to_u64(channel_consistent.pending_requests.len()).unwrap(),
            )]
        }
        FriendMutation::PopFrontPendingRequest => {
            let channel_consistent = if let ChannelStatus::Consistent(channel_consistent) =
                &friend_after.channel_status
            {
                channel_consistent
            } else {
                unreachable!();
            };
            vec![FriendReportMutation::SetNumPendingRequests(
                usize_to_u64(channel_consistent.pending_requests.len()).unwrap(),
            )]
        }
        FriendMutation::PushBackPendingBackwardsOp(_backwards_op) => {
            let channel_consistent = if let ChannelStatus::Consistent(channel_consistent) =
                &friend_after.channel_status
            {
                channel_consistent
            } else {
                unreachable!();
            };
            vec![FriendReportMutation::SetNumPendingBackwardsOps(
                usize_to_u64(channel_consistent.pending_backwards_ops.len()).unwrap(),
            )]
        }
        FriendMutation::PopFrontPendingBackwardsOp => {
            let channel_consistent = if let ChannelStatus::Consistent(channel_consistent) =
                &friend_after.channel_status
            {
                channel_consistent
            } else {
                unreachable!();
            };
            vec![FriendReportMutation::SetNumPendingBackwardsOps(
                usize_to_u64(channel_consistent.pending_backwards_ops.len()).unwrap(),
            )]
        }
        FriendMutation::PushBackPendingUserRequest(_request_send_funds) => {
            let channel_consistent = if let ChannelStatus::Consistent(channel_consistent) =
                &friend_after.channel_status
            {
                channel_consistent
            } else {
                unreachable!();
            };
            vec![FriendReportMutation::SetNumPendingUserRequests(
                usize_to_u64(channel_consistent.pending_user_requests.len()).unwrap(),
            )]
        }
        FriendMutation::PopFrontPendingUserRequest => {
            let channel_consistent = if let ChannelStatus::Consistent(channel_consistent) =
                &friend_after.channel_status
            {
                channel_consistent
            } else {
                unreachable!();
            };
            vec![FriendReportMutation::SetNumPendingUserRequests(
                usize_to_u64(channel_consistent.pending_user_requests.len()).unwrap(),
            )]
        }
        FriendMutation::SetStatus(friend_status) => vec![FriendReportMutation::SetStatus(
            FriendStatusReport::from(friend_status),
        )],
        FriendMutation::SetRemoteRelays(remote_relays) => {
            vec![FriendReportMutation::SetRemoteRelays(remote_relays.clone())]
        }
        FriendMutation::SetName(name) => vec![FriendReportMutation::SetName(name.clone())],
        FriendMutation::SetRate(rate) => vec![FriendReportMutation::SetRate(rate.clone())],
        FriendMutation::SetSentLocalRelays(sent_local_relays) => {
            vec![FriendReportMutation::SetSentLocalRelays(
                sent_local_relays.into(),
            )]
        }
        FriendMutation::SetInconsistent(_) | FriendMutation::SetConsistent(_) => {
            let channel_status_report = ChannelStatusReport::from(&friend_after.channel_status);
            let set_channel_status = FriendReportMutation::SetChannelStatus(channel_status_report);
            let opt_move_token_hashed_report = friend_after
                .channel_status
                .get_last_incoming_move_token_hashed()
                .map(|move_token_hashed| MoveTokenHashedReport::from(&move_token_hashed));
            let set_last_incoming_move_token =
                FriendReportMutation::SetOptLastIncomingMoveToken(opt_move_token_hashed_report);
            vec![set_channel_status, set_last_incoming_move_token]
        }
    }
}

/// Convert a FunderMutation to FunderReportMutation
/// FunderReportMutation are simpler than FunderMutations. They do not require reading the current
/// FunderReport. However, FunderMutations sometimes require access to the current funder_state to
/// make sense. Therefore we require that this function takes FunderState too.
///
/// In the future if we simplify Funder's mutations, we might be able discard the `funder_state`
/// argument here.
pub fn funder_mutation_to_report_mutations<B>(
    funder_mutation: &FunderMutation<B>,
    funder_state: &FunderState<B>,
) -> Vec<FunderReportMutation<B>>
where
    B: Clone + CanonicalSerialize,
{
    let mut funder_state_after = funder_state.clone();
    funder_state_after.mutate(funder_mutation);
    match funder_mutation {
        FunderMutation::FriendMutation((public_key, friend_mutation)) => {
            let friend = funder_state.friends.get(public_key).unwrap();
            friend_mutation_to_report_mutations(&friend_mutation, &friend)
                .into_iter()
                .map(|friend_report_mutation| {
                    FunderReportMutation::PkFriendReportMutation((
                        public_key.clone(),
                        friend_report_mutation,
                    ))
                })
                .collect::<Vec<_>>()
        }
        FunderMutation::AddRelay(named_relay_address) => {
            vec![FunderReportMutation::AddRelay(named_relay_address.clone())]
        }
        FunderMutation::RemoveRelay(public_key) => {
            vec![FunderReportMutation::RemoveRelay(public_key.clone())]
        }
        FunderMutation::AddFriend(add_friend) => {
            let friend_after = funder_state_after
                .friends
                .get(&add_friend.friend_public_key)
                .unwrap();
            let add_friend_report = AddFriendReport {
                friend_public_key: add_friend.friend_public_key.clone(),
                name: add_friend.name.clone(),
                relays: add_friend.relays.clone(),
                opt_last_incoming_move_token: friend_after
                    .channel_status
                    .get_last_incoming_move_token_hashed()
                    .map(|move_token_hashed| MoveTokenHashedReport::from(&move_token_hashed)),
                channel_status: ChannelStatusReport::from(&friend_after.channel_status),
            };
            vec![FunderReportMutation::AddFriend(add_friend_report)]
        }
        FunderMutation::RemoveFriend(friend_public_key) => {
            vec![FunderReportMutation::RemoveFriend(
                friend_public_key.clone(),
            )]
        }
        FunderMutation::AddInvoice(_) | FunderMutation::RemoveInvoice(_) => {
            if funder_state_after.open_invoices.len() != funder_state.open_invoices.len() {
                vec![FunderReportMutation::SetNumOpenInvoices(
                    usize_to_u64(funder_state_after.open_invoices.len()).unwrap(),
                )]
            } else {
                Vec::new()
            }
        }
        FunderMutation::AddIncomingTransaction(_) => vec![],
        FunderMutation::AddTransaction(_) | FunderMutation::RemoveTransaction(_) => {
            if funder_state_after.open_transactions.len() != funder_state.open_transactions.len() {
                vec![FunderReportMutation::SetNumOpenTransactions(
                    usize_to_u64(funder_state_after.open_transactions.len()).unwrap(),
                )]
            } else {
                Vec::new()
            }
        }
        FunderMutation::SetTransactionResponse(_) => vec![],
        FunderMutation::UpdatePayment(_) | FunderMutation::RemovePayment(_) => {
            if funder_state_after.payments.len() != funder_state.payments.len() {
                vec![FunderReportMutation::SetNumPayments(
                    usize_to_u64(funder_state_after.payments.len()).unwrap(),
                )]
            } else {
                Vec::new()
            }
        }
    }
}

pub fn ephemeral_mutation_to_report_mutations<B>(
    ephemeral_mutation: &EphemeralMutation,
    funder_state: &FunderState<B>,
) -> Vec<FunderReportMutation<B>>
where
    B: Clone,
{
    match ephemeral_mutation {
        EphemeralMutation::LivenessMutation(liveness_mutation) => match liveness_mutation {
            LivenessMutation::SetOnline(public_key) => {
                if !funder_state.friends.contains_key(public_key) {
                    // We ignore the liveness mutation if friend does not exist.
                    //
                    // We do this because ephemeral and funder states do not necessarily agree on
                    // the list of friends. It is possible that a friend is marked as online at the
                    // ephemeral, but there is no such friend at the funder.
                    //
                    // Ephemeral represents the most up to date information the funder has received
                    // from the Channeler, while Funder state represents user's configuration.
                    //
                    // The report is always a bit more pessimistic, as a new friend is always
                    // initialized as offline in the report, but it will eventually be marked as
                    // online when this information arrives from the Channeler.
                    return Vec::new();
                }
                let friend_report_mutation =
                    FriendReportMutation::SetLiveness(FriendLivenessReport::Online);
                vec![FunderReportMutation::PkFriendReportMutation((
                    public_key.clone(),
                    friend_report_mutation,
                ))]
            }
            LivenessMutation::SetOffline(public_key) => {
                if !funder_state.friends.contains_key(public_key) {
                    // We ignore the liveness mutation if friend does not exist.
                    return Vec::new();
                }
                let friend_report_mutation =
                    FriendReportMutation::SetLiveness(FriendLivenessReport::Offline);
                vec![FunderReportMutation::PkFriendReportMutation((
                    public_key.clone(),
                    friend_report_mutation,
                ))]
            }
        },
    }
}

#[cfg(test)]
mod test {
    use super::*;

    use std::convert::TryFrom;

    use crate::types::create_hashed;

    use proto::crypto::{PublicKey, RandValue, Signature};
    use proto::funder::messages::{
        BalanceInfo, CountersInfo, Currency, CurrencyBalanceInfo, McInfo, MoveToken, TokenInfo,
    };
    use signature::signature_buff::{
        hash_token_info, move_token_hashed_report_signature_buff, move_token_signature_buff,
    };

    #[test]
    fn test_move_token_signature_buff_sync() {
        let currency = Currency::try_from("FST".to_owned()).unwrap();
        let token_info = TokenInfo {
            mc: McInfo {
                local_public_key: PublicKey::from(&[0; PublicKey::len()]),
                remote_public_key: PublicKey::from(&[1; PublicKey::len()]),
                balances: vec![CurrencyBalanceInfo {
                    currency: currency.clone(),
                    balance_info: BalanceInfo {
                        balance: 5,
                        local_pending_debt: 4,
                        remote_pending_debt: 2,
                    },
                }],
            },
            counters: CountersInfo {
                inconsistency_counter: 3,
                move_token_counter: 7,
            },
        };

        let move_token = MoveToken::<u32> {
            currencies_operations: Vec::new(),
            opt_local_relays: None,
            opt_active_currencies: None,
            info_hash: hash_token_info(&token_info),
            old_token: Signature::from(&[0x55; Signature::len()]),
            rand_nonce: RandValue::from(&[0x66; RandValue::len()]),
            new_token: Signature::from(&[0x77; Signature::len()]),
        };

        let move_token_hashed = create_hashed(&move_token, &token_info);
        let move_token_hashed_report = MoveTokenHashedReport::from(&move_token_hashed);

        // Make sure that we get the same signature buffer from all the different representations
        // of MoveToken:
        let sig_buff = move_token_signature_buff(&move_token);
        let sig_buff_report = move_token_hashed_report_signature_buff(&move_token_hashed_report);

        assert_eq!(sig_buff, sig_buff_report);
        assert_eq!(move_token.new_token, move_token_hashed.new_token);
        assert_eq!(
            move_token_hashed.new_token,
            move_token_hashed_report.new_token
        );
    }
}
