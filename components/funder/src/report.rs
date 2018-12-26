use std::collections::HashMap;
use im::hashmap::HashMap as ImHashMap;

use common::int_convert::usize_to_u64;
use common::safe_arithmetic::{SafeUnsignedArithmetic};

use crypto::identity::PublicKey;

use proto::funder::messages::{RequestsStatus, FriendStatus};

use proto::funder::report::{DirectionReport, FriendLivenessReport, 
    TcReport, ResetTermsReport, ChannelInconsistentReport, ChannelStatusReport, FriendReport,
    FunderReport, FriendReportMutation, AddFriendReport, FunderReportMutation,
    McRequestsStatusReport, McBalanceReport, RequestsStatusReport, FriendStatusReport,
    MoveTokenHashedReport};

use proto::index_client::messages::{IndexMutation, IndexClientState, UpdateFriend};

use crate::types::MoveTokenHashed;

use crate::friend::{FriendState, ChannelStatus, FriendMutation};
use crate::state::{FunderState, FunderMutation};
use crate::mutual_credit::types::{McBalance, McRequestsStatus};
use crate::token_channel::{TokenChannel, TcDirection, TcMutation}; 
use crate::liveness::LivenessMutation;
use crate::ephemeral::{Ephemeral, EphemeralMutation};

#[allow(unused)]
#[derive(Debug)]
pub enum ReportMutateError {
    FriendDoesNotExist,
    FriendAlreadyExists,
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
            operations_hash: move_token_hashed.operations_hash.clone(),
            old_token: move_token_hashed.old_token.clone(),
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

impl From<&TokenChannel> for TcReport {
    fn from(token_channel: &TokenChannel) -> TcReport {
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

impl From<&ChannelStatus> for ChannelStatusReport {
    fn from(channel_status: &ChannelStatus) -> ChannelStatusReport {
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

fn create_friend_report<A: Clone>(friend_state: &FriendState<A>, friend_liveness: &FriendLivenessReport) -> FriendReport<A> {
    let channel_status = ChannelStatusReport::from(&friend_state.channel_status);

    FriendReport {
        address: friend_state.remote_address.clone(),
        name: friend_state.name.clone(),
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

#[allow(unused)]
pub fn create_report<A: Clone>(funder_state: &FunderState<A>, ephemeral: &Ephemeral) -> FunderReport<A> {
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
        opt_address: funder_state.opt_address.clone(),
        friends,
        num_ready_receipts: usize_to_u64(funder_state.ready_receipts.len()).unwrap(),
    }
}


pub fn friend_mutation_to_report_mutations<A: Clone + 'static>(friend_mutation: &FriendMutation<A>,
                                           friend: &FriendState<A>) -> Vec<FriendReportMutation<A>> {

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
                TcMutation::SetTokenWanted => Vec::new(),
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
            vec![FriendReportMutation::SetFriendStatus(FriendStatusReport::from(friend_status))],
        FriendMutation::SetFriendInfo((address, name)) =>
            vec![FriendReportMutation::SetFriendInfo((address.clone(), name.clone()))],
        FriendMutation::SetInconsistent(_) |
        FriendMutation::LocalReset(_) |
        FriendMutation::RemoteReset(_) => {
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
#[allow(unused)]
pub fn funder_mutation_to_report_mutations<A: Clone + 'static>(funder_mutation: &FunderMutation<A>,
                                           funder_state: &FunderState<A>) -> Vec<FunderReportMutation<A>> {

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
        FunderMutation::SetAddress(opt_address) => {
            vec![FunderReportMutation::SetAddress(opt_address.clone())]
        },
        FunderMutation::AddFriend(add_friend) => {
            let friend_after = funder_state_after.friends.get(&add_friend.friend_public_key).unwrap();
            let add_friend_report = AddFriendReport {
                friend_public_key: add_friend.friend_public_key.clone(),
                address: add_friend.address.clone(),
                name: add_friend.name.clone(),
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
        EphemeralMutation::FreezeGuardMutation(_) => Vec::new(),
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

pub fn friend_report_mutate<A>(friend_report: &mut FriendReport<A>, mutation: &FriendReportMutation<A>) 
where   
    A: Clone,
{
    match mutation {
        FriendReportMutation::SetFriendInfo((address, name)) => {
            friend_report.address = address.clone();
            friend_report.name = name.clone();
        },
        FriendReportMutation::SetChannelStatus(channel_status_report) => {
            friend_report.channel_status = channel_status_report.clone();
        },
        FriendReportMutation::SetWantedRemoteMaxDebt(wanted_remote_max_debt) => {
            friend_report.wanted_remote_max_debt = *wanted_remote_max_debt;
        },
        FriendReportMutation::SetWantedLocalRequestsStatus(wanted_local_requests_status) => {
            friend_report.wanted_local_requests_status = wanted_local_requests_status.clone();
        },
        FriendReportMutation::SetNumPendingResponses(num_pending_responses) => {
            friend_report.num_pending_responses = *num_pending_responses;
        },
        FriendReportMutation::SetNumPendingRequests(num_pending_requests) => {
            friend_report.num_pending_requests = *num_pending_requests;
        },
        FriendReportMutation::SetFriendStatus(friend_status) => {
            friend_report.status = friend_status.clone();
        },
        FriendReportMutation::SetNumPendingUserRequests(num_pending_user_requests) => {
            friend_report.num_pending_user_requests = *num_pending_user_requests;
        },
        FriendReportMutation::SetOptLastIncomingMoveToken(opt_last_incoming_move_token) => {
            friend_report.opt_last_incoming_move_token = opt_last_incoming_move_token.clone();
        },
        FriendReportMutation::SetLiveness(friend_liveness_report) => {
            friend_report.liveness = friend_liveness_report.clone();
        },
    }
}

#[allow(unused)]
pub fn funder_report_mutate<A>(funder_report: &mut FunderReport<A>, mutation: &FunderReportMutation<A>) 
    -> Result<(), ReportMutateError> 
where
    A: Clone,
{

    match mutation {
        FunderReportMutation::SetAddress(opt_address) => {
            funder_report.opt_address = opt_address.clone();
            Ok(())
        },
        FunderReportMutation::AddFriend(add_friend_report) => {
            let friend_report = FriendReport {
                address: add_friend_report.address.clone(),
                name: add_friend_report.name.clone(),
                opt_last_incoming_move_token: add_friend_report.opt_last_incoming_move_token.clone(),
                liveness: FriendLivenessReport::Offline,
                channel_status: add_friend_report.channel_status.clone(),
                wanted_remote_max_debt: 0,
                wanted_local_requests_status: RequestsStatusReport::from(&RequestsStatus::Closed),
                num_pending_responses: 0,
                num_pending_requests: 0,
                status: FriendStatusReport::from(&FriendStatus::Disabled),
                num_pending_user_requests: 0,
            };
            if let Some(_) = funder_report.friends.insert(
                add_friend_report.friend_public_key.clone(), friend_report) {

                Err(ReportMutateError::FriendAlreadyExists)
            } else {
                Ok(())
            }
        },
        FunderReportMutation::RemoveFriend(friend_public_key) => {
            if let None = funder_report.friends.remove(&friend_public_key) {
                Err(ReportMutateError::FriendDoesNotExist)
            } else {
                Ok(())
            }
        },
        FunderReportMutation::FriendReportMutation((friend_public_key, friend_report_mutation)) => {
            let mut friend = funder_report.friends.get_mut(friend_public_key)
                .ok_or(ReportMutateError::FriendDoesNotExist)?;
            friend_report_mutate(&mut friend, friend_report_mutation);
            Ok(())
        },
        FunderReportMutation::SetNumReadyReceipts(num_ready_receipts) => {
            funder_report.num_ready_receipts = *num_ready_receipts;
            Ok(())
        },
    }
}


// Conversion to index client mutations and state
// ----------------------------------------------

// This code is used as glue between FunderReport structure and input mutations given to
// `index_client`. This allows the offst-index-client crate to not depend on the offst-funder
// crate.

/// Calculate send and receive capacities for a given `friend_report`.
fn calc_friend_capacities<A>(friend_report: &FriendReport<A>) -> (u128, u128) {
    if friend_report.status == FriendStatusReport::Disabled || 
        friend_report.liveness == FriendLivenessReport::Offline {
        return (0, 0);
    }

    let tc_report = match &friend_report.channel_status {
        ChannelStatusReport::Inconsistent(_) => return (0,0),
        ChannelStatusReport::Consistent(tc_report) => tc_report,
    };

    let balance = &tc_report.balance;

    let send_capacity = if tc_report.requests_status.remote == RequestsStatusReport::Closed {
        0
    } else {
        balance.local_max_debt.saturating_sub(
            balance.local_pending_debt.checked_sub_signed(balance.balance).unwrap())
    };

    let recv_capacity = if tc_report.requests_status.local == RequestsStatusReport::Closed {
        0
    } else {
        balance.remote_max_debt.saturating_sub(
            balance.remote_pending_debt.checked_add_signed(balance.balance).unwrap())
    };

    (send_capacity, recv_capacity)
}

pub fn funder_report_to_index_client_state<A>(funder_report: &FunderReport<A>) -> IndexClientState 
where
    A: Clone,
{
    let friends = funder_report.friends
        .iter()
        .map(|(friend_public_key, friend_report)| 
             (friend_public_key.clone(), calc_friend_capacities(friend_report)))
        .filter(|(_, (send_capacity, recv_capacity))| *send_capacity != 0 || *recv_capacity != 0)
        .collect::<HashMap<PublicKey,(u128, u128)>>();

    IndexClientState {
        friends,
    }
}

pub fn funder_report_mutation_to_index_client_mutation<A>(funder_report: &FunderReport<A>, 
                                                      funder_report_mutation: &FunderReportMutation<A>) -> Option<IndexMutation> 
where
    A: Clone,
{

    let create_update_friend = |public_key: &PublicKey| {
        let mut new_funder_report = funder_report.clone();
        funder_report_mutate(&mut new_funder_report, funder_report_mutation).unwrap();
        
        let new_friend_report = new_funder_report.friends
            .get(public_key)
            .unwrap(); // We assert that a new friend was added

        let (send_capacity, recv_capacity) = calc_friend_capacities(new_friend_report);
        let update_friend = UpdateFriend {
            public_key: public_key.clone(),
            send_capacity,
            recv_capacity,
        };
        IndexMutation::UpdateFriend(update_friend)
    };

    match funder_report_mutation {
        FunderReportMutation::SetAddress(_) | 
        FunderReportMutation::SetNumReadyReceipts(_) => None,
        FunderReportMutation::AddFriend(add_friend_report) => 
            Some(create_update_friend(&add_friend_report.friend_public_key)),
        FunderReportMutation::RemoveFriend(public_key) => Some(IndexMutation::RemoveFriend(public_key.clone())),
        FunderReportMutation::FriendReportMutation((public_key, _friend_report_mutation)) =>
            Some(create_update_friend(&public_key)),
    }
}
