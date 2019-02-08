use std::collections::HashMap;
use im::hashmap::HashMap as ImHashMap;

use common::safe_arithmetic::SafeUnsignedArithmetic;

use crypto::identity::{PublicKey, Signature};
use crypto::hash::HashResult;
use crypto::crypto_rand::RandValue;

use crate::funder::messages::{RequestsStatus, FriendStatus};
use crate::index_server::messages::{IndexMutation, UpdateFriend};
use crate::index_client::messages::IndexClientState;


#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MoveTokenHashedReport {
    pub prefix_hash: HashResult,
    pub local_public_key: PublicKey,
    pub remote_public_key: PublicKey,
    pub inconsistency_counter: u64,
    pub move_token_counter: u128,
    pub balance: i128,
    pub local_pending_debt: u128,
    pub remote_pending_debt: u128,
    pub rand_nonce: RandValue,
    pub new_token: Signature,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub enum SentLocalAddressReport<A> {
    NeverSent,
    Transition((A, A)), // (last sent, before last sent)
    LastSent(A),
}

#[derive(Clone, Serialize, Deserialize, Debug, Eq, PartialEq)]
pub enum FriendStatusReport {
    Enabled,
    Disabled,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, Debug)]
pub enum RequestsStatusReport {
    Open,
    Closed,
}


#[derive(Eq, PartialEq, Clone, Serialize, Deserialize, Debug)]
pub struct McRequestsStatusReport {
    // Local is open/closed for incoming requests:
    pub local: RequestsStatusReport,
    // Remote is open/closed for incoming requests:
    pub remote: RequestsStatusReport,
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct McBalanceReport {
    /// Amount of credits this side has against the remote side.
    /// The other side keeps the negation of this value.
    pub balance: i128,
    /// Maximum possible local debt
    pub local_max_debt: u128,
    /// Maximum possible remote debt
    pub remote_max_debt: u128,
    /// Frozen credits by our side
    pub local_pending_debt: u128,
    /// Frozen credits by the remote side
    pub remote_pending_debt: u128,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DirectionReport {
    Incoming,
    Outgoing,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FriendLivenessReport {
    Online,
    Offline,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TcReport {
    pub direction: DirectionReport,
    pub balance: McBalanceReport,
    pub requests_status: McRequestsStatusReport,
    pub num_local_pending_requests: u64,
    pub num_remote_pending_requests: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResetTermsReport {
    pub reset_token: Signature,
    pub balance_for_reset: i128,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChannelInconsistentReport {
    pub local_reset_terms_balance: i128,
    pub opt_remote_reset_terms: Option<ResetTermsReport>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChannelStatusReport {
    Inconsistent(ChannelInconsistentReport),
    Consistent(TcReport),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FriendReport<A> {
    pub name: String,
    pub remote_address: A, 
    pub sent_local_address: SentLocalAddressReport<A>,
    // Last message signed by the remote side. 
    // Can be used as a proof for the last known balance.
    pub opt_last_incoming_move_token: Option<MoveTokenHashedReport>,
    pub liveness: FriendLivenessReport, // is the friend online/offline?
    pub channel_status: ChannelStatusReport,
    pub wanted_remote_max_debt: u128,
    pub wanted_local_requests_status: RequestsStatusReport,
    pub num_pending_requests: u64,
    pub num_pending_responses: u64,
    // Pending operations to be sent to the token channel.
    pub status: FriendStatusReport,
    pub num_pending_user_requests: u64,
    // Request that the user has sent to this neighbor, 
    // but have not been processed yet. Bounded in size.
}

/// A FunderReport is a summary of a FunderState.
/// It contains the information the Funder exposes to the user apps of the Offst node.
#[derive(Debug, Clone, PartialEq, Eq)]
// TODO: Removed A: Clone here and ImHashMap. Should this struct be cloneable for some reason?
pub struct FunderReport<A: Clone> {
    pub local_public_key: PublicKey,
    pub address: A,
    pub friends: ImHashMap<PublicKey, FriendReport<A>>,
    pub num_ready_receipts: u64,
}

#[allow(unused)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FriendReportMutation<A> {
    SetRemoteAddress(A),
    SetName(String),
    SetSentLocalAddress(SentLocalAddressReport<A>),
    SetChannelStatus(ChannelStatusReport),
    SetWantedRemoteMaxDebt(u128),
    SetWantedLocalRequestsStatus(RequestsStatusReport),
    SetNumPendingRequests(u64),
    SetNumPendingResponses(u64),
    SetStatus(FriendStatusReport),
    SetNumPendingUserRequests(u64),
    SetOptLastIncomingMoveToken(Option<MoveTokenHashedReport>),
    SetLiveness(FriendLivenessReport),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AddFriendReport<A> {
    pub friend_public_key: PublicKey,
    pub name: String,
    pub address: A,
    pub balance: i128, // Initial balance
    pub opt_last_incoming_move_token: Option<MoveTokenHashedReport>,
    pub channel_status: ChannelStatusReport,
}


#[allow(unused)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FunderReportMutation<A> {
    SetAddress(A),
    AddFriend(AddFriendReport<A>),
    RemoveFriend(PublicKey),
    FriendReportMutation((PublicKey, FriendReportMutation<A>)),
    SetNumReadyReceipts(u64),
}


impl From<&FriendStatus> for FriendStatusReport {
    fn from(friend_status: &FriendStatus) -> FriendStatusReport {
        match friend_status {
            FriendStatus::Enabled => FriendStatusReport::Enabled,
            FriendStatus::Disabled => FriendStatusReport::Disabled,
        }
    }
}


impl From<&RequestsStatus> for RequestsStatusReport {
    fn from(requests_status: &RequestsStatus) -> RequestsStatusReport {
        match requests_status {
            RequestsStatus::Open => RequestsStatusReport::Open,
            RequestsStatus::Closed => RequestsStatusReport::Closed,
        }
    }
}

#[allow(unused)]
#[derive(Debug)]
pub enum FunderReportMutateError {
    FriendDoesNotExist,
    FriendAlreadyExists,
}


impl<A> FriendReport<A> 
where
    A: Clone,
{
    pub fn mutate(&mut self, mutation: &FriendReportMutation<A>) {
        match mutation {
            FriendReportMutation::SetRemoteAddress(remote_address) => {
                self.remote_address = remote_address.clone();
            },
            FriendReportMutation::SetName(name) => {
                self.name = name.clone();
            },
            FriendReportMutation::SetSentLocalAddress(sent_local_address_report) => {
                self.sent_local_address = sent_local_address_report.clone();
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
            FriendReportMutation::SetStatus(friend_status) => {
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


impl<A> FunderReport<A> 
where
    A: Clone,
{
    pub fn mutate(&mut self, mutation: &FunderReportMutation<A>) 
        -> Result<(), FunderReportMutateError> {

        match mutation {
            FunderReportMutation::SetAddress(address) => {
                self.address = address.clone();
                Ok(())
            },
            FunderReportMutation::AddFriend(add_friend_report) => {
                let friend_report = FriendReport {
                    remote_address: add_friend_report.address.clone(),
                    name: add_friend_report.name.clone(),
                    sent_local_address: SentLocalAddressReport::NeverSent,
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
                if let Some(_) = self.friends.insert(
                    add_friend_report.friend_public_key.clone(), friend_report) {

                    Err(FunderReportMutateError::FriendAlreadyExists)
                } else {
                    Ok(())
                }
            },
            FunderReportMutation::RemoveFriend(friend_public_key) => {
                if let None = self.friends.remove(&friend_public_key) {
                    Err(FunderReportMutateError::FriendDoesNotExist)
                } else {
                    Ok(())
                }
            },
            FunderReportMutation::FriendReportMutation((friend_public_key, friend_report_mutation)) => {
                let friend = self.friends.get_mut(friend_public_key)
                    .ok_or(FunderReportMutateError::FriendDoesNotExist)?;
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



// Conversion to index client mutations and state
// ----------------------------------------------

// This code is used as glue between FunderReport structure and input mutations given to
// `index_client`. This allows the offst-index-client crate to not depend on the offst-funder
// crate.

// TODO: Maybe this logic shouldn't be here? Where should we move it to?

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

pub fn funder_report_mutation_to_index_mutation<A>(funder_report: &FunderReport<A>, 
                                                      funder_report_mutation: &FunderReportMutation<A>) -> Option<IndexMutation> 
where
    A: Clone,
{

    let create_update_friend = |public_key: &PublicKey| {
        let mut new_funder_report = funder_report.clone();
        new_funder_report.mutate(funder_report_mutation).unwrap();
        
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
