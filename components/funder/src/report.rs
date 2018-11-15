use im::hashmap::HashMap as ImHashMap;

use crypto::identity::PublicKey;
use utils::int_convert::usize_to_u64;

use crate::friend::{FriendState, ChannelStatus, ChannelInconsistent, FriendMutation};
use crate::state::{FunderState, FunderMutation};
use crate::types::{RequestsStatus, FriendStatus, AddFriend, FriendMoveToken};
use crate::mutual_credit::types::{TcBalance, TcRequestsStatus, MutualCredit};
use crate::token_channel::TcDirection; 

#[derive(Clone, Debug)]
pub enum DirectionReport {
    Incoming,
    Outgoing,
}

#[derive(Clone, Debug)]
pub struct TcReport {
    pub direction: DirectionReport,
    pub balance: TcBalance,
    pub requests_status: TcRequestsStatus,
    // Last signed statement from remote side:
    pub opt_last_incoming_move_token: Option<FriendMoveToken>,
}

#[derive(Clone, Debug)]
pub enum ChannelStatusReport {
    Inconsistent(ChannelInconsistent),
    Consistent(TcReport),
}

#[derive(Clone, Debug)]
pub struct FriendReport<A> {
    pub remote_address: A, 
    pub name: String,
    pub channel_status: ChannelStatusReport,
    pub wanted_remote_max_debt: u128,
    pub wanted_local_requests_status: RequestsStatus,
    pub num_pending_responses: u64,
    pub num_pending_requests: u64,
    // Pending operations to be sent to the token channel.
    pub status: FriendStatus,
    pub num_pending_user_requests: u64,
    // Request that the user has sent to this neighbor, 
    // but have not been processed yet. Bounded in size.
}

/// A FunderReport is a summary of a FunderState.
/// It contains the information the Funder exposes to the user apps of the Offst node.
#[derive(Debug)]
pub struct FunderReport<A: Clone> {
    pub friends: ImHashMap<PublicKey, FriendReport<A>>,
    pub num_ready_receipts: usize,
    pub local_public_key: PublicKey,

}

#[allow(unused)]
#[derive(Debug)]
pub enum FriendReportMutation<A> {
    SetFriendInfo((A, String)),
    SetChannelStatus(ChannelStatusReport),
    SetWantedRemoteMaxDebt(u128),
    SetWantedLocalRequestsStatus(RequestsStatus),
    SetNumPendingResponses(u64),
    SetNumPendingRequests(u64),
    SetFriendStatus(FriendStatus),
    SetNumPendingUserRequests(u64),
}

#[allow(unused)]
#[derive(Debug)]
pub enum FunderReportMutation<A> {
    AddFriend(AddFriend<A>),
    RemoveFriend(PublicKey),
    FriendReportMutation((PublicKey, FriendReportMutation<A>)),
    SetNumReadyReceipts(u64),
}

fn create_channel_status_report<A: Clone>(channel_status: &ChannelStatus) -> ChannelStatusReport {
    match channel_status {
        ChannelStatus::Inconsistent(channel_inconsistent) => 
            ChannelStatusReport::Inconsistent(channel_inconsistent.clone()),
        ChannelStatus::Consistent(token_channel) => {
            let direction = match token_channel.get_direction() {
                TcDirection::Incoming(_) => DirectionReport::Incoming,
                TcDirection::Outgoing(_) => DirectionReport::Outgoing,
            };
            let tc_report = TcReport {
                direction,
                balance: token_channel.get_mutual_credit().state().balance.clone(),
                requests_status: token_channel.get_mutual_credit().state().requests_status.clone(),
                opt_last_incoming_move_token: token_channel.get_last_incoming_move_token().cloned(),
            };
            ChannelStatusReport::Consistent(tc_report)
        },
    }
}

fn create_friend_report<A: Clone>(friend_state: &FriendState<A>) -> FriendReport<A> {
    let channel_status = create_channel_status_report::<A>(&friend_state.channel_status);

    FriendReport {
        remote_address: friend_state.remote_address.clone(),
        name: friend_state.name.clone(),
        channel_status,
        wanted_remote_max_debt: friend_state.wanted_remote_max_debt,
        wanted_local_requests_status: friend_state.wanted_local_requests_status.clone(),
        num_pending_responses: usize_to_u64(friend_state.pending_responses.len()).unwrap(),
        num_pending_requests: usize_to_u64(friend_state.pending_requests.len()).unwrap(),
        status: friend_state.status.clone(),
        num_pending_user_requests: usize_to_u64(friend_state.pending_user_requests.len()).unwrap(),
    }
}

pub fn create_report<A: Clone>(funder_state: &FunderState<A>) -> FunderReport<A> {
    let mut friends = ImHashMap::new();
    for (friend_public_key, friend_state) in &funder_state.friends {
        let friend_report = create_friend_report(&friend_state);
        friends.insert(friend_public_key.clone(), friend_report);
    }

    FunderReport {
        friends,
        num_ready_receipts: funder_state.ready_receipts.len(),
        local_public_key: funder_state.local_public_key.clone(),
    }

}

pub fn create_friend_mutation_report<A: Clone + 'static>(friend_mutation: &FriendMutation<A>,
                                           friend: &FriendState<A>) -> Option<FriendReportMutation<A>> {

    let mut friend_after = friend.clone();
    friend_after.mutate(friend_mutation);

    match friend_mutation {
        FriendMutation::TcMutation(tc_mutation) => unimplemented!(), // TODO
        FriendMutation::SetInconsistent(channel_inconsistent) =>
            Some(FriendReportMutation::SetChannelStatus(
                    ChannelStatusReport::Inconsistent(channel_inconsistent.clone()))),
        FriendMutation::SetWantedRemoteMaxDebt(wanted_remote_max_debt) =>
            Some(FriendReportMutation::SetWantedRemoteMaxDebt(*wanted_remote_max_debt)),
        FriendMutation::SetWantedLocalRequestsStatus(requests_status) => 
            Some(FriendReportMutation::SetWantedLocalRequestsStatus(requests_status.clone())),
        FriendMutation::PushBackPendingRequest(_request_send_funds) =>
            Some(FriendReportMutation::SetNumPendingRequests(
                    usize_to_u64(friend_after.pending_requests.len()).unwrap())),
        FriendMutation::PopFrontPendingRequest =>
            Some(FriendReportMutation::SetNumPendingRequests(
                    usize_to_u64(friend_after.pending_requests.len()).unwrap())),
        FriendMutation::PushBackPendingResponse(_response_op) =>
            Some(FriendReportMutation::SetNumPendingResponses(
                    usize_to_u64(friend_after.pending_responses.len()).unwrap())),
        FriendMutation::PopFrontPendingResponse => 
            Some(FriendReportMutation::SetNumPendingResponses(
                    usize_to_u64(friend_after.pending_responses.len()).unwrap())),
        FriendMutation::PushBackPendingUserRequest(_request_send_funds) =>
            Some(FriendReportMutation::SetNumPendingUserRequests(
                    usize_to_u64(friend_after.pending_user_requests.len()).unwrap())),
        FriendMutation::PopFrontPendingUserRequest => 
            Some(FriendReportMutation::SetNumPendingUserRequests(
                    usize_to_u64(friend_after.pending_user_requests.len()).unwrap())),
        FriendMutation::SetStatus(friend_status) => 
            Some(FriendReportMutation::SetFriendStatus(friend_status.clone())),
        FriendMutation::SetFriendInfo((address, name)) =>
            Some(FriendReportMutation::SetFriendInfo((address.clone(), name.clone()))),
        FriendMutation::LocalReset(friend_move_token) => unimplemented!(),  // TODO
        FriendMutation::RemoteReset(friend_move_token) => unimplemented!(), // TODO
    }
}

/// Convert a FunderMutation to FunderReportMutation
/// FunderReportMutation are simpler than FunderMutations. They do not require reading the current
/// FunderReport. However, FunderMutations sometimes require access to the current funder_state to
/// make sense. Therefore we require that this function takes FunderState too.
///
/// In the future if we simplify Funder's mutations, we might be able discard the `funder_state`
/// argument here.
pub fn create_funder_mutation_report<A: Clone + 'static>(funder_mutation: &FunderMutation<A>,
                                           funder_state: &FunderState<A>) -> Option<FunderReportMutation<A>> {

    let mut funder_state_after = funder_state.clone();
    funder_state_after.mutate(funder_mutation);
    match funder_mutation {
        FunderMutation::FriendMutation((public_key, friend_mutation)) => {
            let friend = funder_state.friends.get(public_key).unwrap();
            let friend_report_mutation = create_friend_mutation_report(&friend_mutation, &friend)?;
            Some(FunderReportMutation::FriendReportMutation((public_key.clone(), friend_report_mutation)))
        },
        FunderMutation::AddFriend(add_friend) => {
            Some(FunderReportMutation::AddFriend(add_friend.clone()))
        },
        FunderMutation::RemoveFriend(friend_public_key) => {
            Some(FunderReportMutation::RemoveFriend(friend_public_key.clone()))
        },
        FunderMutation::AddReceipt((uid, receipt)) => {
            if funder_state_after.ready_receipts.len() != funder_state.ready_receipts.len() {
                Some(FunderReportMutation::SetNumReadyReceipts(usize_to_u64(funder_state.ready_receipts.len()).unwrap()))
            } else {
                None
            }
        },
        FunderMutation::RemoveReceipt(uid) => {
            if funder_state_after.ready_receipts.len() != funder_state.ready_receipts.len() {
                Some(FunderReportMutation::SetNumReadyReceipts(usize_to_u64(funder_state.ready_receipts.len()).unwrap()))
            } else {
                None
            }
        },
    }
}

