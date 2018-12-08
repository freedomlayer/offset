use im::hashmap::HashMap as ImHashMap;

use crypto::identity::{PublicKey, Signature};
use common::int_convert::usize_to_u64;

use crate::friend::{FriendState, ChannelStatus, FriendMutation};
use crate::state::{FunderState, FunderMutation};
use crate::types::{RequestsStatus, FriendStatus, MoveTokenHashed};
use crate::mutual_credit::types::{McBalance, McRequestsStatus};
use crate::token_channel::{TokenChannel, TcDirection, TcMutation}; 
use crate::liveness::LivenessMutation;
use crate::ephemeral::{Ephemeral, EphemeralMutation};

#[derive(Clone, Debug)]
pub enum DirectionReport {
    Incoming,
    Outgoing,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FriendLivenessReport {
    Online,
    Offline,
}

#[derive(Clone, Debug)]
pub struct TcReport {
    pub direction: DirectionReport,
    pub balance: McBalance,
    pub requests_status: McRequestsStatus,
    pub num_local_pending_requests: u64,
    pub num_remote_pending_requests: u64,
}

#[derive(Clone, Debug)]
pub struct ResetTermsReport {
    pub reset_token: Signature,
    pub balance_for_reset: i128,
}

#[derive(Clone, Debug)]
pub struct ChannelInconsistentReport {
    pub local_reset_terms_balance: i128,
    pub opt_remote_reset_terms: Option<ResetTermsReport>,
}

#[derive(Clone, Debug)]
pub enum ChannelStatusReport {
    Inconsistent(ChannelInconsistentReport),
    Consistent(TcReport),
}

#[derive(Clone, Debug)]
pub struct FriendReport<A> {
    pub address: A, 
    pub name: String,
    // Last message signed by the remote side. 
    // Can be used as a proof for the last known balance.
    pub opt_last_incoming_move_token: Option<MoveTokenHashed>,
    pub liveness: FriendLivenessReport, // is the friend online/offline?
    pub channel_status: ChannelStatusReport,
    pub wanted_remote_max_debt: u128,
    pub wanted_local_requests_status: RequestsStatus,
    pub num_pending_requests: u64,
    pub num_pending_responses: u64,
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
    pub local_public_key: PublicKey,
    pub opt_address: Option<A>,
    pub friends: ImHashMap<PublicKey, FriendReport<A>>,
    pub num_ready_receipts: u64,
}

#[allow(unused)]
#[derive(Debug)]
pub enum FriendReportMutation<A> {
    SetFriendInfo((A, String)),
    SetChannelStatus(ChannelStatusReport),
    SetWantedRemoteMaxDebt(u128),
    SetWantedLocalRequestsStatus(RequestsStatus),
    SetNumPendingRequests(u64),
    SetNumPendingResponses(u64),
    SetFriendStatus(FriendStatus),
    SetNumPendingUserRequests(u64),
    SetOptLastIncomingMoveToken(Option<MoveTokenHashed>),
    SetLiveness(FriendLivenessReport),
}

#[derive(Clone, Debug)]
pub struct AddFriendReport<A> {
    pub friend_public_key: PublicKey,
    pub address: A,
    pub name: String,
    pub balance: i128, // Initial balance
    pub opt_last_incoming_move_token: Option<MoveTokenHashed>,
    pub channel_status: ChannelStatusReport,
}

#[derive(Debug)]
pub enum ReportMutateError {
    FriendDoesNotExist,
    FriendAlreadyExists,
}


#[allow(unused)]
#[derive(Debug)]
pub enum FunderReportMutation<A> {
    SetAddress(Option<A>),
    AddFriend(AddFriendReport<A>),
    RemoveFriend(PublicKey),
    FriendReportMutation((PublicKey, FriendReportMutation<A>)),
    SetNumReadyReceipts(u64),
}

fn create_token_channel_report(token_channel: &TokenChannel) -> TcReport {
    let direction = match token_channel.get_direction() {
        TcDirection::Incoming(_) => DirectionReport::Incoming,
        TcDirection::Outgoing(_) => DirectionReport::Outgoing,
    };
    let mutual_credit_state = token_channel.get_mutual_credit().state();
    TcReport {
        direction,
        balance: mutual_credit_state.balance.clone(),
        requests_status: mutual_credit_state.requests_status.clone(),
        num_local_pending_requests: usize_to_u64(mutual_credit_state.pending_requests.pending_local_requests.len()).unwrap(),
        num_remote_pending_requests: usize_to_u64(mutual_credit_state.pending_requests.pending_remote_requests.len()).unwrap(),
    }
}

fn create_channel_status_report<A: Clone>(channel_status: &ChannelStatus) -> ChannelStatusReport {
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
            ChannelStatusReport::Consistent(create_token_channel_report(&token_channel)),
    }
}

fn create_friend_report<A: Clone>(friend_state: &FriendState<A>, friend_liveness: &FriendLivenessReport) -> FriendReport<A> {
    let channel_status = create_channel_status_report::<A>(&friend_state.channel_status);

    FriendReport {
        address: friend_state.remote_address.clone(),
        name: friend_state.name.clone(),
        opt_last_incoming_move_token: friend_state.channel_status.get_last_incoming_move_token_hashed(),
        liveness: friend_liveness.clone(),
        channel_status,
        wanted_remote_max_debt: friend_state.wanted_remote_max_debt,
        wanted_local_requests_status: friend_state.wanted_local_requests_status.clone(),
        num_pending_requests: usize_to_u64(friend_state.pending_requests.len()).unwrap(),
        num_pending_responses: usize_to_u64(friend_state.pending_responses.len()).unwrap(),
        status: friend_state.status.clone(),
        num_pending_user_requests: usize_to_u64(friend_state.pending_user_requests.len()).unwrap(),
    }
}

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
                    let channel_status_report = create_channel_status_report::<A>(&friend_after.channel_status);
                    let set_channel_status = FriendReportMutation::SetChannelStatus(channel_status_report);
                    let set_last_incoming_move_token = FriendReportMutation::SetOptLastIncomingMoveToken(
                        friend_after.channel_status.get_last_incoming_move_token_hashed().clone());
                    vec![set_channel_status, set_last_incoming_move_token]
                },
                TcMutation::SetTokenWanted => Vec::new(),
            }
        },
        FriendMutation::SetWantedRemoteMaxDebt(wanted_remote_max_debt) =>
            vec![FriendReportMutation::SetWantedRemoteMaxDebt(*wanted_remote_max_debt)],
        FriendMutation::SetWantedLocalRequestsStatus(requests_status) => 
            vec![FriendReportMutation::SetWantedLocalRequestsStatus(requests_status.clone())],
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
            vec![FriendReportMutation::SetFriendStatus(friend_status.clone())],
        FriendMutation::SetFriendInfo((address, name)) =>
            vec![FriendReportMutation::SetFriendInfo((address.clone(), name.clone()))],
        FriendMutation::SetInconsistent(_) |
        FriendMutation::LocalReset(_) |
        FriendMutation::RemoteReset(_) => {
            let channel_status_report = create_channel_status_report::<A>(&friend_after.channel_status);
            let set_channel_status = FriendReportMutation::SetChannelStatus(channel_status_report);
            let set_last_incoming_move_token = FriendReportMutation::SetOptLastIncomingMoveToken(
                friend_after.channel_status.get_last_incoming_move_token_hashed().clone());
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
                opt_last_incoming_move_token: friend_after.channel_status.get_last_incoming_move_token_hashed().clone(),
                channel_status: create_channel_status_report::<A>(&friend_after.channel_status),
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

impl<A: Clone> FriendReport<A> {
    fn mutate(&mut self, mutation: &FriendReportMutation<A>) {
        match mutation {
            FriendReportMutation::SetFriendInfo((address, name)) => {
                self.address = address.clone();
                self.name = name.clone();
            },
            FriendReportMutation::SetChannelStatus(channel_status_report) => {
                self.channel_status = channel_status_report.clone();
            },
            FriendReportMutation::SetWantedRemoteMaxDebt(wanted_remote_max_debt) => {
                self.wanted_remote_max_debt = *wanted_remote_max_debt;
            },
            FriendReportMutation::SetWantedLocalRequestsStatus(wanted_local_requests_status) => {
                self.wanted_local_requests_status = wanted_local_requests_status.clone();
            },
            FriendReportMutation::SetNumPendingResponses(num_pending_responses) => {
                self.num_pending_responses = *num_pending_responses;
            },
            FriendReportMutation::SetNumPendingRequests(num_pending_requests) => {
                self.num_pending_requests = *num_pending_requests;
            },
            FriendReportMutation::SetFriendStatus(friend_status) => {
                self.status = friend_status.clone();
            },
            FriendReportMutation::SetNumPendingUserRequests(num_pending_user_requests) => {
                self.num_pending_user_requests = *num_pending_user_requests;
            },
            FriendReportMutation::SetOptLastIncomingMoveToken(opt_last_incoming_move_token) => {
                self.opt_last_incoming_move_token = opt_last_incoming_move_token.clone();
            },
            FriendReportMutation::SetLiveness(friend_liveness_report) => {
                self.liveness = friend_liveness_report.clone();
            },
        }
    }
}


impl<A: Clone> FunderReport<A> {
    #[allow(unused)]
    pub fn mutate(&mut self, mutation: &FunderReportMutation<A>) -> Result<(), ReportMutateError> {
        match mutation {
            FunderReportMutation::SetAddress(opt_address) => {
                self.opt_address = opt_address.clone();
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
                    wanted_local_requests_status: RequestsStatus::Closed,
                    num_pending_responses: 0,
                    num_pending_requests: 0,
                    status: FriendStatus::Disable,
                    num_pending_user_requests: 0,
                };
                if let Some(_) = self.friends.insert(
                    add_friend_report.friend_public_key.clone(), friend_report) {

                    Err(ReportMutateError::FriendAlreadyExists)
                } else {
                    Ok(())
                }
            },
            FunderReportMutation::RemoveFriend(friend_public_key) => {
                if let None = self.friends.remove(&friend_public_key) {
                    Err(ReportMutateError::FriendDoesNotExist)
                } else {
                    Ok(())
                }
            },
            FunderReportMutation::FriendReportMutation((friend_public_key, friend_report_mutation)) => {
                let friend = self.friends.get_mut(friend_public_key)
                    .ok_or(ReportMutateError::FriendDoesNotExist)?;
                friend.mutate(friend_report_mutation);
                Ok(())
            },
            FunderReportMutation::SetNumReadyReceipts(num_ready_receipts) => {
                self.num_ready_receipts = *num_ready_receipts;
                Ok(())
            },
        }
    }
}

