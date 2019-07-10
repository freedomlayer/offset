// use im::hashmap::HashMap as ImHashMap;
// use im::vector::Vector as ImVec;

use serde::{Deserialize, Serialize};

// use common::mutable_state::MutableState;

use capnp_conv::{capnp_conv, CapnpConvError, ReadCapnp, WriteCapnp};

use crate::crypto::{HashResult, PublicKey, RandValue, Signature, Uid};

use crate::app_server::messages::{NamedRelayAddress, RelayAddress};
use crate::funder::messages::{FriendStatus, Rate, RequestsStatus};
use crate::net::messages::NetAddress;
use crate::wrapper::Wrapper;

#[capnp_conv(crate::report_capnp::move_token_hashed_report)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MoveTokenHashedReport {
    pub prefix_hash: HashResult,
    pub local_public_key: PublicKey,
    pub remote_public_key: PublicKey,
    pub inconsistency_counter: u64,
    pub move_token_counter: Wrapper<u128>,
    pub balance: Wrapper<i128>,
    pub local_pending_debt: Wrapper<u128>,
    pub remote_pending_debt: Wrapper<u128>,
    pub rand_nonce: RandValue,
    pub new_token: Signature,
}

#[capnp_conv(crate::report_capnp::relays_transition)]
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub struct RelaysTransition<B = NetAddress> {
    pub last_sent: Vec<NamedRelayAddress<B>>,
    pub before_last_sent: Vec<NamedRelayAddress<B>>,
}

#[capnp_conv(crate::report_capnp::sent_local_relays_report)]
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub enum SentLocalRelaysReport<B = NetAddress> {
    NeverSent,
    Transition(RelaysTransition<B>),
    LastSent(Vec<NamedRelayAddress<B>>),
}

#[capnp_conv(crate::report_capnp::friend_status_report)]
#[derive(Clone, Serialize, Deserialize, Debug, Eq, PartialEq)]
pub enum FriendStatusReport {
    Enabled,
    Disabled,
}

#[capnp_conv(crate::report_capnp::requests_status_report)]
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, Debug)]
pub enum RequestsStatusReport {
    Open,
    Closed,
}

#[capnp_conv(crate::report_capnp::mc_requests_status_report)]
#[derive(Eq, PartialEq, Clone, Serialize, Deserialize, Debug)]
pub struct McRequestsStatusReport {
    // Local is open/closed for incoming requests:
    pub local: RequestsStatusReport,
    // Remote is open/closed for incoming requests:
    pub remote: RequestsStatusReport,
}

#[capnp_conv(crate::report_capnp::mc_balance_report)]
#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct McBalanceReport {
    /// Amount of credits this side has against the remote side.
    /// The other side keeps the negation of this value.
    pub balance: Wrapper<i128>,
    /// Maximum possible local debt
    pub local_max_debt: Wrapper<u128>,
    /// Maximum possible remote debt
    pub remote_max_debt: Wrapper<u128>,
    /// Frozen credits by our side
    pub local_pending_debt: Wrapper<u128>,
    /// Frozen credits by the remote side
    pub remote_pending_debt: Wrapper<u128>,
}

#[capnp_conv(crate::report_capnp::direction_report)]
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

#[capnp_conv(crate::report_capnp::friend_liveness_report)]
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

#[capnp_conv(crate::report_capnp::tc_report)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TcReport {
    pub direction: DirectionReport,
    pub balance: McBalanceReport,
    pub requests_status: McRequestsStatusReport,
    pub num_local_pending_requests: u64,
    pub num_remote_pending_requests: u64,
}

#[capnp_conv(crate::report_capnp::reset_terms_report)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResetTermsReport {
    pub reset_token: Signature,
    pub balance_for_reset: Wrapper<i128>,
}

#[capnp_conv(crate::report_capnp::channel_inconsistent_report::opt_remote_reset_terms)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OptRemoteResetTerms {
    RemoteResetTerms(ResetTermsReport),
    Empty,
}

#[capnp_conv(crate::report_capnp::channel_inconsistent_report)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChannelInconsistentReport {
    pub local_reset_terms_balance: Wrapper<i128>,
    pub opt_remote_reset_terms: OptRemoteResetTerms,
}

#[capnp_conv(crate::report_capnp::channel_consistent_report)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChannelConsistentReport {
    pub tc_report: TcReport,
    pub num_pending_requests: u64,
    pub num_pending_backwards_ops: u64,
    pub num_pending_user_requests: u64,
}

#[capnp_conv(crate::report_capnp::channel_status_report)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChannelStatusReport {
    Inconsistent(ChannelInconsistentReport),
    Consistent(ChannelConsistentReport),
}

#[capnp_conv(crate::report_capnp::opt_last_incoming_move_token)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OptLastIncomingMoveToken {
    MoveTokenHashed(MoveTokenHashedReport),
    Empty,
}

#[capnp_conv(crate::report_capnp::friend_report)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FriendReport<B = NetAddress> {
    pub name: String,
    pub rate: Rate,
    pub remote_relays: Vec<RelayAddress<B>>,
    pub sent_local_relays: SentLocalRelaysReport<B>,
    // Last message signed by the remote side.
    // Can be used as a proof for the last known balance.
    pub opt_last_incoming_move_token: OptLastIncomingMoveToken,
    // TODO: The state of liveness = true with status = disabled should never happen.
    // Can we somehow express this in the type system?
    pub liveness: FriendLivenessReport, // is the friend online/offline?
    pub channel_status: ChannelStatusReport,
    pub wanted_remote_max_debt: Wrapper<u128>,
    pub wanted_local_requests_status: RequestsStatusReport,
    pub status: FriendStatusReport,
}

#[capnp_conv(crate::report_capnp::pk_friend_report)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PkFriendReport<B = NetAddress> {
    pub friend_public_key: PublicKey,
    pub friend_report: FriendReport<B>,
}

/// A FunderReport is a summary of a FunderState.
/// It contains the information the Funder exposes to the user apps of the Offst node.
#[capnp_conv(crate::report_capnp::funder_report)]
#[derive(Debug, Clone, PartialEq, Eq)]
// TODO: Removed A: Clone here and ImHashMap. Should this struct be cloneable for some reason?
pub struct FunderReport<B = NetAddress> {
    pub local_public_key: PublicKey,
    pub relays: Vec<NamedRelayAddress<B>>,
    pub friends: Vec<PkFriendReport<B>>,
    pub num_open_invoices: u64,
    pub num_payments: u64,
    pub num_open_transactions: u64,
}

#[allow(clippy::large_enum_variant)]
// #[capnp_conv(crate::report_capnp::friend_report_mutation)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FriendReportMutation<B = NetAddress> {
    SetRemoteRelays(Vec<RelayAddress<B>>),
    SetName(String),
    SetRate(Rate),
    SetSentLocalRelays(SentLocalRelaysReport<B>),
    SetChannelStatus(ChannelStatusReport),
    SetWantedRemoteMaxDebt(Wrapper<u128>),
    SetWantedLocalRequestsStatus(RequestsStatusReport),
    SetNumPendingRequests(u64),
    SetNumPendingBackwardsOps(u64),
    SetNumPendingUserRequests(u64),
    SetStatus(FriendStatusReport),
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

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FunderReportMutation<B = NetAddress> {
    AddRelay(NamedRelayAddress<B>),
    RemoveRelay(PublicKey),
    AddFriend(AddFriendReport<B>),
    RemoveFriend(PublicKey),
    FriendReportMutation((PublicKey, FriendReportMutation<B>)),
    SetNumOpenInvoices(u64),
    SetNumPayments(u64),
    SetNumOpenTransactions(u64),
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

/*
 *
// TODO: Restore later

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
            FriendReportMutation::SetRate(rate) => {
                self.rate = rate.clone();
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
            FriendReportMutation::SetNumPendingBackwardsOps(num_pending_backwards_ops) => {
                if let ChannelStatusReport::Consistent(channel_consistent_report) =
                    &mut self.channel_status
                {
                    channel_consistent_report.num_pending_backwards_ops =
                        *num_pending_backwards_ops;
                } else {
                    unreachable!();
                }
            }
            FriendReportMutation::SetNumPendingRequests(num_pending_requests) => {
                if let ChannelStatusReport::Consistent(channel_consistent_report) =
                    &mut self.channel_status
                {
                    channel_consistent_report.num_pending_requests = *num_pending_requests;
                } else {
                    unreachable!();
                }
            }
            FriendReportMutation::SetNumPendingUserRequests(num_pending_user_requests) => {
                if let ChannelStatusReport::Consistent(channel_consistent_report) =
                    &mut self.channel_status
                {
                    channel_consistent_report.num_pending_user_requests =
                        *num_pending_user_requests;
                } else {
                    unreachable!();
                }
            }
            FriendReportMutation::SetStatus(friend_status) => {
                self.status = friend_status.clone();
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
                self.relays.push(named_relay_address.clone());
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
                    rate: Rate::new(),
                    remote_relays: add_friend_report.relays.clone(),
                    sent_local_relays: SentLocalRelaysReport::NeverSent,
                    opt_last_incoming_move_token: add_friend_report
                        .opt_last_incoming_move_token
                        .clone(),
                    liveness: FriendLivenessReport::Offline,
                    channel_status: add_friend_report.channel_status.clone(),
                    wanted_remote_max_debt: 0.into(),
                    wanted_local_requests_status: RequestsStatusReport::from(
                        &RequestsStatus::Closed,
                    ),
                    status: FriendStatusReport::from(&FriendStatus::Disabled),
                };
                if self
                    .friends
                    .insert(add_friend_report.friend_public_key.clone(), friend_report)
                    .is_some()
                {
                    Err(FunderReportMutateError::FriendAlreadyExists)
                } else {
                    Ok(())
                }
            }
            FunderReportMutation::RemoveFriend(friend_public_key) => {
                if self.friends.remove(&friend_public_key).is_none() {
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
            FunderReportMutation::SetNumOpenInvoices(num_open_invoices) => {
                self.num_open_invoices = *num_open_invoices;
                Ok(())
            }
            FunderReportMutation::SetNumPayments(num_payments) => {
                self.num_payments = *num_payments;
                Ok(())
            }
            FunderReportMutation::SetNumOpenTransactions(num_open_transactions) => {
                self.num_open_transactions = *num_open_transactions;
                Ok(())
            }
        }
    }
}
*/
