use im::hashmap::HashMap as ImHashMap;
use im::vector::Vector as ImVec;

use common::mutable_state::MutableState;

use crypto::crypto_rand::RandValue;
use crypto::hash::HashResult;
use crypto::identity::{PublicKey, Signature};
use crypto::uid::Uid;

use crate::app_server::messages::{NamedRelayAddress, RelayAddress};
use crate::funder::messages::{FriendStatus, RequestsStatus};
use crate::net::messages::NetAddress;

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
pub enum SentLocalRelaysReport<B = NetAddress>
where
    B: Clone,
{
    NeverSent,
    Transition((ImVec<NamedRelayAddress<B>>, ImVec<NamedRelayAddress<B>>)), // (last sent, before last sent)
    LastSent(ImVec<NamedRelayAddress<B>>),
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

impl DirectionReport {
    pub fn is_incoming(&self) -> bool {
        if let DirectionReport::Incoming = self {
            true
        } else {
            false
        }
    }

    pub fn is_outgoing(&self) -> bool {
        if let DirectionReport::Outgoing = self {
            true
        } else {
            false
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FriendLivenessReport {
    Online,
    Offline,
}

impl FriendLivenessReport {
    pub fn is_online(&self) -> bool {
        if let FriendLivenessReport::Online = self {
            true
        } else {
            false
        }
    }
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
pub struct FriendReport<B = NetAddress>
where
    B: Clone,
{
    pub name: String,
    pub remote_relays: Vec<RelayAddress<B>>,
    pub sent_local_relays: SentLocalRelaysReport<B>,
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
pub struct FunderReport<B = NetAddress>
where
    B: Clone,
{
    pub local_public_key: PublicKey,
    pub relays: ImVec<NamedRelayAddress<B>>,
    pub friends: ImHashMap<PublicKey, FriendReport<B>>,
    pub num_ready_receipts: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FriendReportMutation<B = NetAddress>
where
    B: Clone,
{
    SetRemoteRelays(Vec<RelayAddress<B>>),
    SetName(String),
    SetSentLocalRelays(SentLocalRelaysReport<B>),
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
pub struct AddFriendReport<B = NetAddress> {
    pub friend_public_key: PublicKey,
    pub name: String,
    pub relays: Vec<RelayAddress<B>>,
    pub balance: i128, // Initial balance
    pub opt_last_incoming_move_token: Option<MoveTokenHashedReport>,
    pub channel_status: ChannelStatusReport,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FunderReportMutation<B = NetAddress>
where
    B: Clone,
{
    AddRelay(NamedRelayAddress<B>),
    RemoveRelay(PublicKey),
    AddFriend(AddFriendReport<B>),
    RemoveFriend(PublicKey),
    FriendReportMutation((PublicKey, FriendReportMutation<B>)),
    SetNumReadyReceipts(u64),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunderReportMutations<B: Clone> {
    pub opt_app_request_id: Option<Uid>,
    pub mutations: Vec<FunderReportMutation<B>>,
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

#[derive(Debug)]
pub enum FunderReportMutateError {
    FriendDoesNotExist,
    FriendAlreadyExists,
}

impl<B> MutableState for FriendReport<B>
where
    B: Clone,
{
    type Mutation = FriendReportMutation<B>;
    type MutateError = !;

    fn mutate(&mut self, mutation: &Self::Mutation) -> Result<(), Self::MutateError> {
        match mutation {
            FriendReportMutation::SetName(name) => {
                self.name = name.clone();
            }
            FriendReportMutation::SetRemoteRelays(remote_relays) => {
                self.remote_relays = remote_relays.clone();
            }
            FriendReportMutation::SetSentLocalRelays(sent_local_relays_report) => {
                self.sent_local_relays = sent_local_relays_report.clone();
            }
            FriendReportMutation::SetChannelStatus(channel_status_report) => {
                self.channel_status = channel_status_report.clone();
            }
            FriendReportMutation::SetWantedRemoteMaxDebt(wanted_remote_max_debt) => {
                self.wanted_remote_max_debt = *wanted_remote_max_debt;
            }
            FriendReportMutation::SetWantedLocalRequestsStatus(wanted_local_requests_status) => {
                self.wanted_local_requests_status = wanted_local_requests_status.clone();
            }
            FriendReportMutation::SetNumPendingResponses(num_pending_responses) => {
                self.num_pending_responses = *num_pending_responses;
            }
            FriendReportMutation::SetNumPendingRequests(num_pending_requests) => {
                self.num_pending_requests = *num_pending_requests;
            }
            FriendReportMutation::SetStatus(friend_status) => {
                self.status = friend_status.clone();
            }
            FriendReportMutation::SetNumPendingUserRequests(num_pending_user_requests) => {
                self.num_pending_user_requests = *num_pending_user_requests;
            }
            FriendReportMutation::SetOptLastIncomingMoveToken(opt_last_incoming_move_token) => {
                self.opt_last_incoming_move_token = opt_last_incoming_move_token.clone();
            }
            FriendReportMutation::SetLiveness(friend_liveness_report) => {
                self.liveness = friend_liveness_report.clone();
            }
        };
        Ok(())
    }
}

impl<B> MutableState for FunderReport<B>
where
    B: Clone,
{
    type Mutation = FunderReportMutation<B>;
    type MutateError = FunderReportMutateError;

    fn mutate(&mut self, mutation: &Self::Mutation) -> Result<(), Self::MutateError> {
        match mutation {
            FunderReportMutation::AddRelay(named_relay_address) => {
                // Remove duplicates:
                self.relays.retain(|cur_named_relay_address| {
                    cur_named_relay_address.public_key != named_relay_address.public_key
                });
                // Insert:
                self.relays.push_back(named_relay_address.clone());
                Ok(())
            }
            FunderReportMutation::RemoveRelay(public_key) => {
                self.relays.retain(|cur_named_relay_address| {
                    &cur_named_relay_address.public_key != public_key
                });
                Ok(())
            }
            FunderReportMutation::AddFriend(add_friend_report) => {
                let friend_report = FriendReport {
                    name: add_friend_report.name.clone(),
                    remote_relays: add_friend_report.relays.clone(),
                    sent_local_relays: SentLocalRelaysReport::NeverSent,
                    opt_last_incoming_move_token: add_friend_report
                        .opt_last_incoming_move_token
                        .clone(),
                    liveness: FriendLivenessReport::Offline,
                    channel_status: add_friend_report.channel_status.clone(),
                    wanted_remote_max_debt: 0,
                    wanted_local_requests_status: RequestsStatusReport::from(
                        &RequestsStatus::Closed,
                    ),
                    num_pending_responses: 0,
                    num_pending_requests: 0,
                    status: FriendStatusReport::from(&FriendStatus::Disabled),
                    num_pending_user_requests: 0,
                };
                if let Some(_) = self
                    .friends
                    .insert(add_friend_report.friend_public_key.clone(), friend_report)
                {
                    Err(FunderReportMutateError::FriendAlreadyExists)
                } else {
                    Ok(())
                }
            }
            FunderReportMutation::RemoveFriend(friend_public_key) => {
                if let None = self.friends.remove(&friend_public_key) {
                    Err(FunderReportMutateError::FriendDoesNotExist)
                } else {
                    Ok(())
                }
            }
            FunderReportMutation::FriendReportMutation((
                friend_public_key,
                friend_report_mutation,
            )) => {
                let friend = self
                    .friends
                    .get_mut(friend_public_key)
                    .ok_or(FunderReportMutateError::FriendDoesNotExist)?;
                friend.mutate(friend_report_mutation)?;
                Ok(())
            }
            FunderReportMutation::SetNumReadyReceipts(num_ready_receipts) => {
                self.num_ready_receipts = *num_ready_receipts;
                Ok(())
            }
        }
    }
}
