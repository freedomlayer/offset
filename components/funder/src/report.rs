use im::hashmap::HashMap as ImHashMap;

use common::int_convert::usize_to_u64;
use common::canonical_serialize::CanonicalSerialize;

use proto::report::messages::{DirectionReport, FriendLivenessReport, 
    TcReport, ResetTermsReport, ChannelInconsistentReport, ChannelStatusReport, FriendReport,
    FunderReport, FriendReportMutation, AddFriendReport, FunderReportMutation,
    McRequestsStatusReport, McBalanceReport, RequestsStatusReport, FriendStatusReport,
    MoveTokenHashedReport, SentLocalAddressReport};

use crate::types::MoveTokenHashed;

use crate::friend::{FriendState, ChannelStatus, FriendMutation, 
    SentLocalAddress};
use crate::state::{FunderState, FunderMutation};
use crate::mutual_credit::types::{McBalance, McRequestsStatus};
use crate::token_channel::{TokenChannel, TcDirection, TcMutation}; 
use crate::liveness::LivenessMutation;
use crate::ephemeral::{Ephemeral, EphemeralMutation};



impl<A> Into<SentLocalAddressReport<A>> for &SentLocalAddress<A> 
where
    A: Clone,
{
    fn into(self) -> SentLocalAddressReport<A> {
        match self {
            SentLocalAddress::NeverSent => 
                SentLocalAddressReport::NeverSent,
            SentLocalAddress::Transition(t) => 
                SentLocalAddressReport::Transition(t.clone()),
            SentLocalAddress::LastSent(address) => 
                SentLocalAddressReport::LastSent(address.clone()),
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
            local_public_key: move_token_hashed.local_public_key.clone(),
            remote_public_key: move_token_hashed.remote_public_key.clone(),
            inconsistency_counter: move_token_hashed.inconsistency_counter,
            move_token_counter: move_token_hashed.move_token_counter,
            balance: move_token_hashed.balance,
            local_pending_debt: move_token_hashed.local_pending_debt,
            remote_pending_debt: move_token_hashed.remote_pending_debt,
            rand_nonce: move_token_hashed.rand_nonce.clone(),
            new_token: move_token_hashed.new_token.clone(),
        }
    }
}

impl<A> From<&TokenChannel<A>> for TcReport 
where
    A: CanonicalSerialize + Clone,
{
    fn from(token_channel: &TokenChannel<A>) -> TcReport {
        let direction = match token_channel.get_direction() {
            TcDirection::Incoming(_) => DirectionReport::Incoming,
            TcDirection::Outgoing(_) => DirectionReport::Outgoing,
        };
        let mutual_credit_state = token_channel.get_mutual_credit().state();
        TcReport {
            direction,
            balance: McBalanceReport::from(&mutual_credit_state.balance),
            requests_status: McRequestsStatusReport::from(&mutual_credit_state.requests_status),
            num_local_pending_requests: usize_to_u64(mutual_credit_state.pending_requests.pending_local_requests.len()).unwrap(),
            num_remote_pending_requests: usize_to_u64(mutual_credit_state.pending_requests.pending_remote_requests.len()).unwrap(),
        }
    }
}

impl<A> From<&ChannelStatus<A>> for ChannelStatusReport 
where
    A: CanonicalSerialize + Clone,
{
    fn from(channel_status: &ChannelStatus<A>) -> ChannelStatusReport {
        match channel_status {
            ChannelStatus::Inconsistent(channel_inconsistent) => {
                let opt_remote_reset_terms = channel_inconsistent.opt_remote_reset_terms
                    .clone()
                    .map(|remote_reset_terms|
                        ResetTermsReport {
                            reset_token: remote_reset_terms.reset_token.clone(),
                            balance_for_reset: remote_reset_terms.balance_for_reset,
                        }
                    );
                let channel_inconsistent_report = ChannelInconsistentReport {
                    local_reset_terms_balance: channel_inconsistent.local_reset_terms.balance_for_reset,
                    opt_remote_reset_terms,
                };
                ChannelStatusReport::Inconsistent(channel_inconsistent_report)
            },
            ChannelStatus::Consistent(token_channel) =>
                ChannelStatusReport::Consistent(TcReport::from(token_channel)),
        }
    }
}

fn create_friend_report<A>(friend_state: &FriendState<A>, friend_liveness: &FriendLivenessReport) -> FriendReport<A> 
where
    A: CanonicalSerialize + Clone,
{
    let channel_status = ChannelStatusReport::from(&friend_state.channel_status);

    FriendReport {
        name: friend_state.name.clone(),
        remote_address: friend_state.remote_address.clone(),
        sent_local_address: (&friend_state.sent_local_address).into(),
        opt_last_incoming_move_token: friend_state.channel_status.get_last_incoming_move_token_hashed()
            .map(|move_token_hashed| MoveTokenHashedReport::from(&move_token_hashed)),
        liveness: friend_liveness.clone(),
        channel_status,
        wanted_remote_max_debt: friend_state.wanted_remote_max_debt,
        wanted_local_requests_status: RequestsStatusReport::from(&friend_state.wanted_local_requests_status),
        num_pending_requests: usize_to_u64(friend_state.pending_requests.len()).unwrap(),
        num_pending_responses: usize_to_u64(friend_state.pending_responses.len()).unwrap(),
        status: FriendStatusReport::from(&friend_state.status),
        num_pending_user_requests: usize_to_u64(friend_state.pending_user_requests.len()).unwrap(),
    }
}

pub fn create_report<A>(funder_state: &FunderState<A>, ephemeral: &Ephemeral) -> FunderReport<A> 
where
    A: CanonicalSerialize + Clone,
{
    let mut friends = ImHashMap::new();
    for (friend_public_key, friend_state) in &funder_state.friends {
        let friend_liveness = match ephemeral.liveness.is_online(friend_public_key) {
            true => FriendLivenessReport::Online,
            false => FriendLivenessReport::Offline,
        };
        let friend_report = create_friend_report(&friend_state, &friend_liveness);
        friends.insert(friend_public_key.clone(), friend_report);
    }

    FunderReport {
        local_public_key: funder_state.local_public_key.clone(),
        address: funder_state.address.clone(),
        friends,
        num_ready_receipts: usize_to_u64(funder_state.ready_receipts.len()).unwrap(),
    }
}

pub fn create_initial_report<A>(funder_state: &FunderState<A>) -> FunderReport<A> 
where
    A: CanonicalSerialize + Clone,
{
    create_report(funder_state, &Ephemeral::new())
}


pub fn friend_mutation_to_report_mutations<A>(friend_mutation: &FriendMutation<A>,
                                           friend: &FriendState<A>) -> Vec<FriendReportMutation<A>> 
where
    A: CanonicalSerialize + Clone,
{

    let mut friend_after = friend.clone();
    friend_after.mutate(friend_mutation);
    match friend_mutation {
        FriendMutation::TcMutation(tc_mutation) => {
            match tc_mutation {
                TcMutation::McMutation(_) |
                TcMutation::SetDirection(_) => {
                    let channel_status_report = ChannelStatusReport::from(&friend_after.channel_status);
                    let set_channel_status = FriendReportMutation::SetChannelStatus(channel_status_report);
                    let set_last_incoming_move_token = FriendReportMutation::SetOptLastIncomingMoveToken(
                        friend_after.channel_status.get_last_incoming_move_token_hashed()
                            .map(|move_token_hashed| MoveTokenHashedReport::from(&move_token_hashed)));
                    vec![set_channel_status, set_last_incoming_move_token]
                },
            }
        },
        FriendMutation::SetWantedRemoteMaxDebt(wanted_remote_max_debt) =>
            vec![FriendReportMutation::SetWantedRemoteMaxDebt(*wanted_remote_max_debt)],
        FriendMutation::SetWantedLocalRequestsStatus(requests_status) => 
            vec![FriendReportMutation::SetWantedLocalRequestsStatus(RequestsStatusReport::from(requests_status))],
        FriendMutation::PushBackPendingRequest(_request_send_funds) =>
            vec![FriendReportMutation::SetNumPendingRequests(
                    usize_to_u64(friend_after.pending_requests.len()).unwrap())],
        FriendMutation::PopFrontPendingRequest =>
            vec![FriendReportMutation::SetNumPendingRequests(
                    usize_to_u64(friend_after.pending_requests.len()).unwrap())],
        FriendMutation::PushBackPendingResponse(_response_op) =>
            vec![FriendReportMutation::SetNumPendingResponses(
                    usize_to_u64(friend_after.pending_responses.len()).unwrap())],
        FriendMutation::PopFrontPendingResponse => 
            vec![FriendReportMutation::SetNumPendingResponses(
                    usize_to_u64(friend_after.pending_responses.len()).unwrap())],
        FriendMutation::PushBackPendingUserRequest(_request_send_funds) =>
            vec![FriendReportMutation::SetNumPendingUserRequests(
                    usize_to_u64(friend_after.pending_user_requests.len()).unwrap())],
        FriendMutation::PopFrontPendingUserRequest => 
            vec![FriendReportMutation::SetNumPendingUserRequests(
                    usize_to_u64(friend_after.pending_user_requests.len()).unwrap())],
        FriendMutation::SetStatus(friend_status) => 
            vec![FriendReportMutation::SetStatus(FriendStatusReport::from(friend_status))],
        FriendMutation::SetRemoteAddress(remote_address) =>
            vec![FriendReportMutation::SetRemoteAddress(remote_address.clone())],
        FriendMutation::SetName(name) =>
            vec![FriendReportMutation::SetName(name.clone())],
        FriendMutation::SetSentLocalAddress(sent_local_address) =>
            vec![FriendReportMutation::SetSentLocalAddress(sent_local_address.into())],
        FriendMutation::SetInconsistent(_) |
        FriendMutation::SetConsistent(_) => {
            let channel_status_report = ChannelStatusReport::from(&friend_after.channel_status);
            let set_channel_status = FriendReportMutation::SetChannelStatus(channel_status_report);
            let opt_move_token_hashed_report = friend_after.channel_status.get_last_incoming_move_token_hashed()
                .map(|move_token_hashed| MoveTokenHashedReport::from(&move_token_hashed));
            let set_last_incoming_move_token = FriendReportMutation::SetOptLastIncomingMoveToken(
                opt_move_token_hashed_report);
            vec![set_channel_status, set_last_incoming_move_token]
        },
    }
}

// TODO: How to add liveness mutation?

/// Convert a FunderMutation to FunderReportMutation
/// FunderReportMutation are simpler than FunderMutations. They do not require reading the current
/// FunderReport. However, FunderMutations sometimes require access to the current funder_state to
/// make sense. Therefore we require that this function takes FunderState too.
///
/// In the future if we simplify Funder's mutations, we might be able discard the `funder_state`
/// argument here.
pub fn funder_mutation_to_report_mutations<A>(funder_mutation: &FunderMutation<A>,
                                           funder_state: &FunderState<A>) -> Vec<FunderReportMutation<A>> 
where
    A: CanonicalSerialize + Clone,
{

    let mut funder_state_after = funder_state.clone();
    funder_state_after.mutate(funder_mutation);
    match funder_mutation {
        FunderMutation::FriendMutation((public_key, friend_mutation)) => {
            let friend = funder_state.friends.get(public_key).unwrap();
            friend_mutation_to_report_mutations(&friend_mutation, &friend)
                .into_iter()
                .map(|friend_report_mutation| 
                     FunderReportMutation::FriendReportMutation((public_key.clone(), friend_report_mutation)))
                .collect::<Vec<_>>()
        },
        FunderMutation::SetAddress(address) => {
            vec![FunderReportMutation::SetAddress(address.clone())]
        },
        FunderMutation::AddFriend(add_friend) => {
            let friend_after = funder_state_after.friends.get(&add_friend.friend_public_key).unwrap();
            let add_friend_report = AddFriendReport {
                friend_public_key: add_friend.friend_public_key.clone(),
                name: add_friend.name.clone(),
                address: add_friend.address.clone(),
                balance: add_friend.balance.clone(), // Initial balance
                opt_last_incoming_move_token: friend_after.channel_status.get_last_incoming_move_token_hashed()
                    .map(|move_token_hashed| MoveTokenHashedReport::from(&move_token_hashed)),
                channel_status: ChannelStatusReport::from(&friend_after.channel_status),
            };
            vec![FunderReportMutation::AddFriend(add_friend_report)]
        },
        FunderMutation::RemoveFriend(friend_public_key) => {
            vec![FunderReportMutation::RemoveFriend(friend_public_key.clone())]
        },
        FunderMutation::AddReceipt((_uid, _receipt)) => {
            if funder_state_after.ready_receipts.len() != funder_state.ready_receipts.len() {
                vec![FunderReportMutation::SetNumReadyReceipts(usize_to_u64(funder_state_after.ready_receipts.len()).unwrap())]
            } else {
                Vec::new()
            }
        },
        FunderMutation::RemoveReceipt(_uid) => {
            if funder_state_after.ready_receipts.len() != funder_state.ready_receipts.len() {
                vec![FunderReportMutation::SetNumReadyReceipts(usize_to_u64(funder_state_after.ready_receipts.len()).unwrap())]
            } else {
                Vec::new()
            }
        },
    }
}

pub fn ephemeral_mutation_to_report_mutations<A: Clone>(ephemeral_mutation: &EphemeralMutation) 
                -> Vec<FunderReportMutation<A>> {

    match ephemeral_mutation {
        EphemeralMutation::LivenessMutation(liveness_mutation) => {
            match liveness_mutation {
                LivenessMutation::SetOnline(public_key) => {
                    let friend_report_mutation = FriendReportMutation::SetLiveness(FriendLivenessReport::Online);
                    vec![FunderReportMutation::FriendReportMutation((public_key.clone(), friend_report_mutation))]
                },
                LivenessMutation::SetOffline(public_key) => {
                    let friend_report_mutation = FriendReportMutation::SetLiveness(FriendLivenessReport::Offline);
                    vec![FunderReportMutation::FriendReportMutation((public_key.clone(), friend_report_mutation))]
                },
            }
        },
    }
}

