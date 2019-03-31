use im::vector::Vector as ImVec;
use std::fmt::Debug;

use crypto::identity::PublicKey;

use common::canonical_serialize::CanonicalSerialize;
use common::safe_arithmetic::SafeUnsignedArithmetic;

use proto::app_server::messages::{NamedRelayAddress, RelayAddress};
use proto::funder::messages::{
    FailureSendFunds, FriendStatus, PendingRequest, RequestSendFunds, RequestsStatus, ResetTerms,
    ResponseSendFunds,
};

use crate::token_channel::{TcMutation, TokenChannel};
use crate::types::MoveTokenHashed;

#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum ResponseOp {
    Response(ResponseSendFunds),
    UnsignedResponse(PendingRequest),
    Failure(FailureSendFunds),
    UnsignedFailure(PendingRequest),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
                .map(|named_relay_address| named_relay_address.into())
                .collect::<Vec<_>>(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FriendMutation<B: Clone> {
    TcMutation(TcMutation<B>),
    SetInconsistent(ChannelInconsistent),
    SetConsistent(TokenChannel<B>),
    SetWantedRemoteMaxDebt(u128),
    SetWantedLocalRequestsStatus(RequestsStatus),
    PushBackPendingRequest(RequestSendFunds),
    PopFrontPendingRequest,
    PushBackPendingResponse(ResponseOp),
    PopFrontPendingResponse,
    PushBackPendingUserRequest(RequestSendFunds),
    PopFrontPendingUserRequest,
    SetStatus(FriendStatus),
    SetRemoteRelays(Vec<RelayAddress<B>>),
    SetName(String),
    SetSentLocalRelays(SentLocalRelays<B>),
}

#[derive(PartialEq, Eq, Clone, Serialize, Deserialize, Debug)]
pub struct ChannelInconsistent {
    pub opt_last_incoming_move_token: Option<MoveTokenHashed>,
    pub local_reset_terms: ResetTerms,
    pub opt_remote_reset_terms: Option<ResetTerms>,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum ChannelStatus<B> {
    Inconsistent(ChannelInconsistent),
    Consistent(TokenChannel<B>),
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
            ChannelStatus::Consistent(token_channel) => {
                token_channel.get_last_incoming_move_token_hashed().cloned()
            }
        }
    }
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct FriendState<B: Clone> {
    pub local_public_key: PublicKey,
    pub remote_public_key: PublicKey,
    pub remote_relays: Vec<RelayAddress<B>>,
    pub sent_local_relays: SentLocalRelays<B>,
    pub name: String,
    pub channel_status: ChannelStatus<B>,
    pub wanted_remote_max_debt: u128,
    pub wanted_local_requests_status: RequestsStatus,
    pub pending_requests: ImVec<RequestSendFunds>,
    pub pending_responses: ImVec<ResponseOp>,
    // Pending operations to be sent to the token channel.
    pub status: FriendStatus,
    pub pending_user_requests: ImVec<RequestSendFunds>,
    // Request that the user has sent to this neighbor,
    // but have not been processed yet. Bounded in size.
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
        balance: i128,
    ) -> Self {
        let token_channel = TokenChannel::new(local_public_key, remote_public_key, balance);

        FriendState {
            local_public_key: local_public_key.clone(),
            remote_public_key: remote_public_key.clone(),
            remote_relays,
            sent_local_relays: SentLocalRelays::NeverSent,
            name,
            channel_status: ChannelStatus::Consistent(token_channel),

            // The remote_max_debt we want to have. When possible, this will be sent to the remote
            // side.
            wanted_remote_max_debt: 0,
            wanted_local_requests_status: RequestsStatus::Closed,
            // The local_send_price we want to have (Or possibly close requests, by having an empty
            // send price). When possible, this will be updated with the TokenChannel.
            pending_requests: ImVec::new(),
            pending_responses: ImVec::new(),
            status: FriendStatus::Disabled,
            pending_user_requests: ImVec::new(),
        }
    }

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

    pub fn mutate(&mut self, friend_mutation: &FriendMutation<B>) {
        match friend_mutation {
            FriendMutation::TcMutation(tc_mutation) => match &mut self.channel_status {
                ChannelStatus::Consistent(ref mut token_channel) => {
                    token_channel.mutate(tc_mutation)
                }
                ChannelStatus::Inconsistent(_) => unreachable!(),
            },
            FriendMutation::SetInconsistent(channel_inconsistent) => {
                self.channel_status = ChannelStatus::Inconsistent(channel_inconsistent.clone());
            }
            FriendMutation::SetConsistent(token_channel) => {
                self.channel_status = ChannelStatus::Consistent(token_channel.clone());
            }
            FriendMutation::SetWantedRemoteMaxDebt(wanted_remote_max_debt) => {
                self.wanted_remote_max_debt = *wanted_remote_max_debt;
            }
            FriendMutation::SetWantedLocalRequestsStatus(wanted_local_requests_status) => {
                self.wanted_local_requests_status = wanted_local_requests_status.clone();
            }
            FriendMutation::PushBackPendingRequest(request_send_funds) => {
                self.pending_requests.push_back(request_send_funds.clone());
            }
            FriendMutation::PopFrontPendingRequest => {
                let _ = self.pending_requests.pop_front();
            }
            FriendMutation::PushBackPendingResponse(response_op) => {
                self.pending_responses.push_back(response_op.clone());
            }
            FriendMutation::PopFrontPendingResponse => {
                let _ = self.pending_responses.pop_front();
            }
            FriendMutation::PushBackPendingUserRequest(request_send_funds) => {
                self.pending_user_requests
                    .push_back(request_send_funds.clone());
            }
            FriendMutation::PopFrontPendingUserRequest => {
                let _ = self.pending_user_requests.pop_front();
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
