use im::vector::Vector as ImVec;
use std::collections::HashMap as ImHashMap;
use std::fmt::Debug;

use common::ser_utils::{SerBase64, SerMapStrAny, SerString};

use signature::canonical::CanonicalSerialize;

use proto::app_server::messages::{NamedRelayAddress, RelayAddress};
use proto::crypto::PublicKey;
use proto::funder::messages::{
    CancelSendFundsOp, CollectSendFundsOp, Currency, FriendStatus, Rate, RequestSendFundsOp,
    ResetTerms, ResponseSendFundsOp,
};

use crate::token_channel::{TcMutation, TokenChannel};
use crate::types::MoveTokenHashed;

/// Any operation that goes backwards (With respect to the initial request)
#[derive(Arbitrary, Clone, Serialize, Deserialize, PartialEq, Eq, Debug)]
pub enum BackwardsOp {
    Response(ResponseSendFundsOp),
    Cancel(CancelSendFundsOp),
    Collect(CollectSendFundsOp),
}

#[derive(Arbitrary, Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SentLocalRelays<B>
where
    B: Clone,
{
    NeverSent,
    Transition((ImVec<NamedRelayAddress<B>>, ImVec<NamedRelayAddress<B>>)), // (last sent, before last sent)
    LastSent(ImVec<NamedRelayAddress<B>>),
}

impl<B> SentLocalRelays<B>
where
    B: Clone + Debug,
{
    pub fn to_vec(&self) -> Vec<RelayAddress<B>> {
        match self {
            SentLocalRelays::NeverSent => Vec::new(),
            SentLocalRelays::Transition((last_relays, prev_last_relays)) => {
                // Create a unique list of all relay public keys:
                let mut relays: Vec<RelayAddress<B>> = Vec::new();
                for relay in last_relays {
                    relays.push(relay.clone().into());
                }
                for relay in prev_last_relays {
                    relays.push(relay.clone().into());
                }
                // Note: a vector must be sorted in order to use dedup_by_key()!
                relays.sort_by_key(|relay_address| relay_address.public_key.clone());
                relays.dedup_by_key(|relay_address| relay_address.public_key.clone());
                relays
            }
            SentLocalRelays::LastSent(last_address) => last_address
                .iter()
                .cloned()
                .map(Into::into)
                .collect::<Vec<_>>(),
        }
    }
}
#[derive(Arbitrary, PartialEq, Eq, Clone, Serialize, Deserialize, Debug)]
pub struct ChannelInconsistent {
    pub opt_last_incoming_move_token: Option<MoveTokenHashed>,
    pub local_reset_terms: ResetTerms,
    pub opt_remote_reset_terms: Option<ResetTerms>,
}

#[derive(Arbitrary, PartialEq, Eq, Clone, Serialize, Deserialize, Debug)]
pub struct ChannelConsistent<B> {
    /// Our mutual state with the remote side
    pub token_channel: TokenChannel<B>,
    /// A queue of requests that need to be sent to the remote friend
    pub pending_requests: ImVec<(Currency, RequestSendFundsOp)>,
    /// A queue of backwards operations (Response, Cancel, Commit) that need to be sent to the remote side
    /// We keep backwards op on a separate queue because those operations are not supposed to fail
    /// (While requests may fail due to lack of trust for example)
    pub pending_backwards_ops: ImVec<(Currency, BackwardsOp)>,
    /// Pending requests originating from the user.
    /// We care more about these requests, because those are payments that our user wants to make.
    /// This queue should be bounded in size (TODO: Check this)
    pub pending_user_requests: ImVec<(Currency, RequestSendFundsOp)>,
}

#[allow(clippy::large_enum_variant)]
#[derive(Arbitrary, Clone, Serialize, Deserialize, Debug, PartialEq, Eq)]
pub enum ChannelStatus<B> {
    Inconsistent(ChannelInconsistent),
    Consistent(ChannelConsistent<B>),
}

impl<B> ChannelStatus<B>
where
    B: Clone + CanonicalSerialize,
{
    pub fn get_last_incoming_move_token_hashed(&self) -> Option<MoveTokenHashed> {
        match &self {
            ChannelStatus::Inconsistent(channel_inconsistent) => {
                channel_inconsistent.opt_last_incoming_move_token.clone()
            }
            ChannelStatus::Consistent(channel_consistent) => channel_consistent
                .token_channel
                .get_last_incoming_move_token_hashed()
                .cloned(),
        }
    }
}

#[derive(Arbitrary, Clone, Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct CurrencyConfig {
    /// Rate of forwarding transactions that arrived from this friend to any other friend
    /// for a certain currency.
    pub rate: Rate,
    /// Credit frame for the remote side (Set by the user of this node)
    /// The remote side does not know this value.
    #[serde(with = "SerString")]
    pub remote_max_debt: u128,
    /// Can new requests be sent through the mutual credit with this friend?
    pub is_open: bool,
}

#[derive(Arbitrary, Clone, Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct FriendState<B: Clone> {
    /// Public key of this node
    #[serde(with = "SerBase64")]
    pub local_public_key: PublicKey,
    /// Public key of the friend node
    #[serde(with = "SerBase64")]
    pub remote_public_key: PublicKey,
    /// Relays on which the friend node can be found.
    /// This list of relays corresponds to the last report of relays we got from the remote friend.
    pub remote_relays: Vec<RelayAddress<B>>,
    /// The last list of our used relays we have sent to the remote friend.
    /// We maintain this list to deal with relays drift.
    pub sent_local_relays: SentLocalRelays<B>,
    /// Locally maintained name of the remote friend node.
    pub name: String,
    /// Local configurations for currencies relationship with this friend
    #[serde(with = "SerMapStrAny")]
    pub currency_configs: ImHashMap<Currency, CurrencyConfig>,
    /// Friend status. If disabled, we don't attempt to connect to this friend. (Friend will think
    /// we are offline).
    pub status: FriendStatus,
    /// Mutual credit channel information
    pub channel_status: ChannelStatus<B>,
}

#[allow(clippy::large_enum_variant)]
#[derive(Arbitrary, Debug, Clone)]
pub enum FriendMutation<B: Clone> {
    TcMutation(TcMutation<B>),
    SetInconsistent(ChannelInconsistent),
    SetConsistent(TokenChannel<B>),
    UpdateCurrencyConfig((Currency, CurrencyConfig)),
    RemoveCurrencyConfig(Currency),
    PushBackPendingRequest((Currency, RequestSendFundsOp)),
    PopFrontPendingRequest,
    PushBackPendingBackwardsOp((Currency, BackwardsOp)),
    PopFrontPendingBackwardsOp,
    PushBackPendingUserRequest((Currency, RequestSendFundsOp)),
    PopFrontPendingUserRequest,
    RemovePendingRequestsCurrency(Currency),
    RemovePendingUserRequestsCurrency(Currency),
    RemovePendingRequests,
    SetStatus(FriendStatus),
    SetRemoteRelays(Vec<RelayAddress<B>>),
    SetName(String),
    SetSentLocalRelays(SentLocalRelays<B>),
}

impl CurrencyConfig {
    pub fn new() -> Self {
        Self {
            rate: Rate::new(),
            remote_max_debt: 0,
            is_open: false,
        }
    }
}

impl<B> FriendState<B>
where
    B: Clone + CanonicalSerialize,
{
    pub fn new(
        local_public_key: &PublicKey,
        remote_public_key: &PublicKey,
        remote_relays: Vec<RelayAddress<B>>,
        name: String,
    ) -> Self {
        let channel_consistent = ChannelConsistent {
            token_channel: TokenChannel::new(local_public_key, remote_public_key),
            pending_requests: ImVec::new(),
            pending_backwards_ops: ImVec::new(),
            pending_user_requests: ImVec::new(),
        };

        FriendState {
            local_public_key: local_public_key.clone(),
            remote_public_key: remote_public_key.clone(),
            remote_relays,
            sent_local_relays: SentLocalRelays::NeverSent,
            name,
            currency_configs: ImHashMap::new(),
            status: FriendStatus::Disabled,
            channel_status: ChannelStatus::Consistent(channel_consistent),
        }
    }

    /*
    // TODO: Do we use this function somewhere?
    /// Find the shared credits we have with this friend.
    /// This value is used for freeze guard calculations.
    /// This value is the capacity shared between the rest of the friends.
    ///
    /// ```text
    ///         ---B
    ///        /
    /// A--*--O-----C
    ///        \
    ///         ---D
    /// ```
    /// In the picture above, the shared credits between O and A will be shared between the nodes
    /// B, C and D.
    ///
    pub fn get_shared_credits(&self) -> u128 {
        let balance = match &self.channel_status {
            ChannelStatus::Consistent(token_channel) => {
                &token_channel.get_mutual_credit().state().balance
            }
            ChannelStatus::Inconsistent(_channel_inconsistent) => return 0,
        };
        balance
            .local_max_debt
            .saturating_add_signed(balance.balance)
    }
    */

    pub fn mutate(&mut self, friend_mutation: &FriendMutation<B>) {
        match friend_mutation {
            FriendMutation::TcMutation(tc_mutation) => match &mut self.channel_status {
                ChannelStatus::Consistent(ref mut channel_consistent) => {
                    channel_consistent.token_channel.mutate(tc_mutation)
                }
                ChannelStatus::Inconsistent(_) => unreachable!(),
            },
            FriendMutation::SetInconsistent(channel_inconsistent) => {
                self.channel_status = ChannelStatus::Inconsistent(channel_inconsistent.clone());
            }
            FriendMutation::SetConsistent(token_channel) => {
                let channel_consistent = ChannelConsistent {
                    token_channel: token_channel.clone(),
                    pending_requests: ImVec::new(),
                    pending_backwards_ops: ImVec::new(),
                    pending_user_requests: ImVec::new(),
                };
                self.channel_status = ChannelStatus::Consistent(channel_consistent);
            }
            FriendMutation::UpdateCurrencyConfig((currency, currency_config)) => {
                let _ = self
                    .currency_configs
                    .insert(currency.clone(), currency_config.clone());
            }
            FriendMutation::RemoveCurrencyConfig(currency) => {
                let channel_consistent = match &mut self.channel_status {
                    ChannelStatus::Consistent(ref mut channel_consistent) => channel_consistent,
                    ChannelStatus::Inconsistent(_) => unreachable!(),
                };
                // We can not remove configuration for a currency that is currency in use.
                if channel_consistent
                    .token_channel
                    .get_active_currencies()
                    .calc_active()
                    .contains(currency)
                {
                    unreachable!();
                }
                let _ = self.currency_configs.remove(currency);
            }
            FriendMutation::PushBackPendingRequest((currency, request_send_funds)) => {
                if let ChannelStatus::Consistent(channel_consistent) = &mut self.channel_status {
                    channel_consistent
                        .pending_requests
                        .push_back((currency.clone(), request_send_funds.clone()));
                } else {
                    unreachable!();
                }
            }
            FriendMutation::PopFrontPendingRequest => {
                if let ChannelStatus::Consistent(channel_consistent) = &mut self.channel_status {
                    channel_consistent.pending_requests.pop_front();
                } else {
                    unreachable!();
                }
            }
            FriendMutation::PushBackPendingBackwardsOp((currency, backwards_op)) => {
                if let ChannelStatus::Consistent(channel_consistent) = &mut self.channel_status {
                    channel_consistent
                        .pending_backwards_ops
                        .push_back((currency.clone(), backwards_op.clone()));
                } else {
                    unreachable!();
                }
            }
            FriendMutation::PopFrontPendingBackwardsOp => {
                if let ChannelStatus::Consistent(channel_consistent) = &mut self.channel_status {
                    channel_consistent.pending_backwards_ops.pop_front();
                } else {
                    unreachable!();
                }
            }
            FriendMutation::PushBackPendingUserRequest((currency, request_send_funds)) => {
                if let ChannelStatus::Consistent(channel_consistent) = &mut self.channel_status {
                    channel_consistent
                        .pending_user_requests
                        .push_back((currency.clone(), request_send_funds.clone()));
                } else {
                    unreachable!();
                }
            }
            FriendMutation::PopFrontPendingUserRequest => {
                if let ChannelStatus::Consistent(channel_consistent) = &mut self.channel_status {
                    channel_consistent.pending_user_requests.pop_front();
                } else {
                    unreachable!();
                }
            }
            FriendMutation::RemovePendingRequestsCurrency(currency) => {
                // Remove all pending outgoing messages for a certain currency.
                if let ChannelStatus::Consistent(channel_consistent) = &mut self.channel_status {
                    channel_consistent
                        .pending_requests
                        .retain(|(currency0, _)| currency0 != currency);
                } else {
                    unreachable!();
                }
            }
            FriendMutation::RemovePendingUserRequestsCurrency(currency) => {
                // Remove all pending outgoing messages for a certain currency.
                if let ChannelStatus::Consistent(channel_consistent) = &mut self.channel_status {
                    channel_consistent
                        .pending_user_requests
                        .retain(|(currency0, _)| currency0 != currency);
                } else {
                    unreachable!();
                }
            }
            FriendMutation::RemovePendingRequests => {
                if let ChannelStatus::Consistent(channel_consistent) = &mut self.channel_status {
                    channel_consistent.pending_requests = ImVec::new();
                    channel_consistent.pending_user_requests = ImVec::new();
                } else {
                    unreachable!();
                }
            }
            FriendMutation::SetStatus(friend_status) => {
                self.status = friend_status.clone();
            }
            FriendMutation::SetRemoteRelays(remote_relays) => {
                self.remote_relays = remote_relays.clone();
            }
            FriendMutation::SetName(friend_name) => {
                self.name = friend_name.clone();
            }
            FriendMutation::SetSentLocalRelays(sent_local_relays) => {
                self.sent_local_relays = sent_local_relays.clone();
            }
        }
    }
}
