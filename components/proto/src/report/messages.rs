// use std::collections::HashMap as ImHashMap;
// use im::vector::Vector as ImVec;
use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use common::mutable_state::MutableState;
use common::never::Never;
use common::ser_utils::ser_string;

use capnp_conv::{capnp_conv, CapnpConvError, ReadCapnp, WriteCapnp};

use crate::crypto::{HashResult, PublicKey, RandValue, Signature, Uid};

use crate::app_server::messages::{NamedRelayAddress, RelayAddress};
use crate::funder::messages::{
    Currency, CurrencyBalance, FriendStatus, Rate, RequestsStatus, TokenInfo,
};
use crate::net::messages::NetAddress;
use crate::wrapper::Wrapper;

#[capnp_conv(crate::report_capnp::move_token_hashed_report)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MoveTokenHashedReport {
    pub prefix_hash: HashResult,
    pub token_info: TokenInfo,
    pub rand_nonce: RandValue,
    pub new_token: Signature,
}

#[capnp_conv(crate::report_capnp::friend_status_report)]
#[derive(Arbitrary, Clone, Serialize, Deserialize, Debug, Eq, PartialEq)]
pub enum FriendStatusReport {
    Enabled,
    Disabled,
}

#[capnp_conv(crate::report_capnp::requests_status_report)]
#[derive(Arbitrary, Clone, PartialEq, Eq, Serialize, Deserialize, Debug)]
pub enum RequestsStatusReport {
    Open,
    Closed,
}

#[capnp_conv(crate::report_capnp::mc_balance_report)]
#[derive(Arbitrary, Clone, Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct McBalanceReport {
    /// Amount of credits this side has against the remote side.
    /// The other side keeps the negation of this value.
    #[capnp_conv(with = Wrapper<i128>)]
    #[serde(with = "ser_string")]
    pub balance: i128,
    /// Frozen credits by our side
    #[capnp_conv(with = Wrapper<u128>)]
    #[serde(with = "ser_string")]
    pub local_pending_debt: u128,
    /// Frozen credits by the remote side
    #[capnp_conv(with = Wrapper<u128>)]
    #[serde(with = "ser_string")]
    pub remote_pending_debt: u128,
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

#[capnp_conv(crate::report_capnp::currency_report)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CurrencyReport {
    pub currency: Currency,
    pub balance: McBalanceReport,
}

#[capnp_conv(crate::report_capnp::reset_terms_report)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResetTermsReport {
    pub reset_token: Signature,
    pub balance_for_reset: Vec<CurrencyBalance>,
}

#[capnp_conv(crate::report_capnp::channel_inconsistent_report::opt_remote_reset_terms)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OptRemoteResetTerms {
    RemoteResetTerms(ResetTermsReport),
    Empty,
}

// TODO: Replace with a macro:
impl From<Option<ResetTermsReport>> for OptRemoteResetTerms {
    fn from(opt: Option<ResetTermsReport>) -> Self {
        match opt {
            Some(reset_terms_report) => OptRemoteResetTerms::RemoteResetTerms(reset_terms_report),
            None => OptRemoteResetTerms::Empty,
        }
    }
}

impl From<OptRemoteResetTerms> for Option<ResetTermsReport> {
    fn from(opt: OptRemoteResetTerms) -> Self {
        match opt {
            OptRemoteResetTerms::RemoteResetTerms(reset_terms_report) => Some(reset_terms_report),
            OptRemoteResetTerms::Empty => None,
        }
    }
}

#[capnp_conv(crate::report_capnp::channel_inconsistent_report)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChannelInconsistentReport {
    pub local_reset_terms: Vec<CurrencyBalance>,
    #[capnp_conv(with = OptRemoteResetTerms)]
    pub opt_remote_reset_terms: Option<ResetTermsReport>,
}

#[capnp_conv(crate::report_capnp::channel_consistent_report)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChannelConsistentReport {
    pub currency_reports: Vec<CurrencyReport>,
}

#[capnp_conv(crate::report_capnp::channel_status_report)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChannelStatusReport {
    Inconsistent(ChannelInconsistentReport),
    Consistent(ChannelConsistentReport),
}

#[allow(clippy::large_enum_variant)]
#[capnp_conv(crate::report_capnp::opt_last_incoming_move_token)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OptLastIncomingMoveToken {
    MoveTokenHashed(MoveTokenHashedReport),
    Empty,
}

// TODO: Replace with a macro:
impl From<Option<MoveTokenHashedReport>> for OptLastIncomingMoveToken {
    fn from(opt: Option<MoveTokenHashedReport>) -> Self {
        match opt {
            Some(move_token_hashed_report) => {
                OptLastIncomingMoveToken::MoveTokenHashed(move_token_hashed_report)
            }
            None => OptLastIncomingMoveToken::Empty,
        }
    }
}

impl From<OptLastIncomingMoveToken> for Option<MoveTokenHashedReport> {
    fn from(opt: OptLastIncomingMoveToken) -> Self {
        match opt {
            OptLastIncomingMoveToken::MoveTokenHashed(move_token_hashed_report) => {
                Some(move_token_hashed_report)
            }
            OptLastIncomingMoveToken::Empty => None,
        }
    }
}

#[capnp_conv(crate::report_capnp::currency_config_report)]
#[derive(Arbitrary, Clone, Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct CurrencyConfigReport {
    pub currency: Currency,
    /// Rate of forwarding transactions that arrived from this friend to any other friend
    /// for a certain currency.
    pub rate: Rate,
    /// Credit frame for the remote side (Set by the user of this node)
    #[capnp_conv(with = Wrapper<u128>)]
    #[serde(with = "ser_string")]
    pub remote_max_debt: u128,
    /// Can requests be sent through this mutual credit?
    pub is_open: bool,
}

#[capnp_conv(crate::report_capnp::friend_report)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FriendReport<B = NetAddress> {
    pub name: String,
    pub remote_relays: Vec<RelayAddress<B>>,
    pub currency_configs: Vec<CurrencyConfigReport>,
    // Last message signed by the remote side.
    // Can be used as a proof for the last known balance.
    #[capnp_conv(with = OptLastIncomingMoveToken)]
    pub opt_last_incoming_move_token: Option<MoveTokenHashedReport>,
    // TODO: The state of liveness = true with status = disabled should never happen.
    // Can we somehow express this in the type system?
    pub liveness: FriendLivenessReport, // is the friend online/offline?
    pub channel_status: ChannelStatusReport,
    pub status: FriendStatusReport,
}

#[capnp_conv(crate::report_capnp::pk_friend_report)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PkFriendReport<B = NetAddress> {
    pub friend_public_key: PublicKey,
    pub friend_report: FriendReport<B>,
}

#[capnp_conv(crate::report_capnp::pk_friend_report_list)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PkFriendReportList {
    list: Vec<PkFriendReport<NetAddress>>,
}

impl From<PkFriendReportList> for HashMap<PublicKey, FriendReport<NetAddress>> {
    fn from(friends_vec: PkFriendReportList) -> Self {
        friends_vec
            .list
            .into_iter()
            .map(|pk_friend_report| {
                (
                    pk_friend_report.friend_public_key,
                    pk_friend_report.friend_report,
                )
            })
            .collect()
    }
}

impl From<HashMap<PublicKey, FriendReport<NetAddress>>> for PkFriendReportList {
    fn from(hash_map: HashMap<PublicKey, FriendReport<NetAddress>>) -> Self {
        PkFriendReportList {
            list: hash_map
                .into_iter()
                .map(|(friend_public_key, friend_report)| PkFriendReport {
                    friend_public_key,
                    friend_report,
                })
                .collect(),
        }
    }
}

/// A FunderReport is a summary of a FunderState.
/// It contains the information the Funder exposes to the user apps of the Offset node.
#[capnp_conv(crate::report_capnp::funder_report)]
#[derive(Debug, Clone, PartialEq, Eq)]
// TODO: Removed A: Clone here and ImHashMap. Should this struct be cloneable for some reason?
pub struct FunderReport<B = NetAddress> {
    pub local_public_key: PublicKey,
    pub relays: Vec<NamedRelayAddress<B>>,
    #[capnp_conv(with = PkFriendReportList)]
    pub friends: HashMap<PublicKey, FriendReport<B>>,
}

#[allow(clippy::large_enum_variant)]
#[capnp_conv(crate::report_capnp::friend_report_mutation)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FriendReportMutation<B = NetAddress> {
    SetRemoteRelays(Vec<RelayAddress<B>>),
    SetName(String),
    UpdateCurrencyConfig(CurrencyConfigReport),
    RemoveCurrencyConfig(Currency),
    SetChannelStatus(ChannelStatusReport),
    SetStatus(FriendStatusReport),
    #[capnp_conv(with = OptLastIncomingMoveToken)]
    SetOptLastIncomingMoveToken(Option<MoveTokenHashedReport>),
    SetLiveness(FriendLivenessReport),
}

#[capnp_conv(crate::report_capnp::add_friend_report)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AddFriendReport<B = NetAddress> {
    pub friend_public_key: PublicKey,
    pub name: String,
    pub relays: Vec<RelayAddress<B>>,
    #[capnp_conv(with = OptLastIncomingMoveToken)]
    pub opt_last_incoming_move_token: Option<MoveTokenHashedReport>,
    pub channel_status: ChannelStatusReport,
}

#[capnp_conv(crate::report_capnp::pk_friend_report_mutation)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PkFriendReportMutation<B = NetAddress> {
    friend_public_key: PublicKey,
    friend_report_mutation: FriendReportMutation<B>,
}

impl From<PkFriendReportMutation> for (PublicKey, FriendReportMutation) {
    fn from(input: PkFriendReportMutation) -> Self {
        (input.friend_public_key, input.friend_report_mutation)
    }
}

impl From<(PublicKey, FriendReportMutation)> for PkFriendReportMutation {
    fn from(
        (friend_public_key, friend_report_mutation): (PublicKey, FriendReportMutation),
    ) -> Self {
        Self {
            friend_public_key,
            friend_report_mutation,
        }
    }
}

#[allow(clippy::large_enum_variant)]
#[capnp_conv(crate::report_capnp::funder_report_mutation)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FunderReportMutation<B = NetAddress> {
    AddRelay(NamedRelayAddress<B>),
    RemoveRelay(PublicKey),
    AddFriend(AddFriendReport<B>),
    RemoveFriend(PublicKey),
    #[capnp_conv(with = PkFriendReportMutation<NetAddress>)]
    PkFriendReportMutation((PublicKey, FriendReportMutation<B>)),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunderReportMutations<B> {
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
    type MutateError = Never;

    fn mutate(&mut self, mutation: &Self::Mutation) -> Result<(), Self::MutateError> {
        match mutation {
            FriendReportMutation::SetName(name) => {
                self.name = name.clone();
            }
            FriendReportMutation::UpdateCurrencyConfig(currency_config_report) => {
                if let Some(pos) = self
                    .currency_configs
                    .iter()
                    .position(|c_c| c_c.currency == currency_config_report.currency)
                {
                    self.currency_configs[pos] = currency_config_report.clone();
                } else {
                    // Not found:
                    self.currency_configs.push(currency_config_report.clone());
                    // Canonicalize:
                    self.currency_configs
                        .sort_by(|c_c1, c_c2| c_c1.currency.cmp(&c_c2.currency));
                }
            }
            FriendReportMutation::RemoveCurrencyConfig(currency) => {
                self.currency_configs
                    .retain(|c_c| c_c.currency != *currency);
            }

            FriendReportMutation::SetRemoteRelays(remote_relays) => {
                self.remote_relays = remote_relays.clone();
            }
            FriendReportMutation::SetChannelStatus(channel_status_report) => {
                self.channel_status = channel_status_report.clone();
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
                    currency_configs: Vec::new(),
                    remote_relays: add_friend_report.relays.clone(),
                    opt_last_incoming_move_token: add_friend_report
                        .opt_last_incoming_move_token
                        .clone(),
                    liveness: FriendLivenessReport::Offline,
                    channel_status: add_friend_report.channel_status.clone(),
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
            FunderReportMutation::PkFriendReportMutation((
                friend_public_key,
                friend_report_mutation,
            )) => {
                let friend = self
                    .friends
                    .get_mut(friend_public_key)
                    .ok_or(FunderReportMutateError::FriendDoesNotExist)?;
                friend
                    .mutate(friend_report_mutation)
                    .map_err(|_| unreachable!())?;
                Ok(())
            }
        }
    }
}
