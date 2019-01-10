use std::hash::Hash;
use im::vector::Vector;

use common::safe_arithmetic::SafeUnsignedArithmetic;
use common::canonical_serialize::CanonicalSerialize;

use proto::funder::messages::{RequestSendFunds,
    ResponseSendFunds, FailureSendFunds, ResetTerms,
    FriendStatus, RequestsStatus, PendingRequest,
    SignedResponse, SignedFailure, TPublicKey};

use crate::token_channel::{TcMutation, TokenChannel};
use crate::types::MoveTokenHashed;



#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum ResponseOp<P,RS,FS> {
    Response(SignedResponse<RS>),
    UnsignedResponse(PendingRequest<P>),
    Failure(SignedFailure<P,FS>),
    UnsignedFailure(PendingRequest<P>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SentLocalAddress<A> {
    NeverSent,
    Transition((A, A)), // (last sent, before last sent)
    LastSent(A),
}

impl<A> SentLocalAddress<A> 
where
    A: Clone,
{
    pub fn to_vec(&self) -> Vec<A> {
        match self {
            SentLocalAddress::NeverSent => Vec::new(),
            SentLocalAddress::Transition((last_address, prev_last_address)) =>
                vec![last_address.clone(), prev_last_address.clone()],
            SentLocalAddress::LastSent(last_address) =>
                vec![last_address.clone()],
        }
    }
}


#[allow(unused)]
#[derive(Debug)]
pub enum FriendMutation<A,P:Clone,RS,FS,MS> {
    TcMutation(TcMutation<A,P,RS,FS,MS>),
    SetInconsistent(ChannelInconsistent<P,MS>),
    SetConsistent(TokenChannel<A,P,RS,FS,MS>),
    SetWantedRemoteMaxDebt(u128),
    SetWantedLocalRequestsStatus(RequestsStatus),
    PushBackPendingRequest(RequestSendFunds<P>),
    PopFrontPendingRequest,
    PushBackPendingResponse(ResponseOp<P,RS,FS>),
    PopFrontPendingResponse,
    PushBackPendingUserRequest(RequestSendFunds<P>),
    PopFrontPendingUserRequest,
    SetStatus(FriendStatus),
    SetRemoteAddress(A),
    SetName(String),
    SetSentLocalAddress(SentLocalAddress<A>),
    // LocalReset(UnsignedMoveToken<A>),
    // The outgoing move token message we have sent to reset the channel.
    // RemoteReset(MoveToken<A>),
}

#[derive(PartialEq, Eq, Clone, Serialize, Deserialize, Debug)]
pub struct ChannelInconsistent<P,MS> {
    pub opt_last_incoming_move_token: Option<MoveTokenHashed<P,MS>>,
    pub local_reset_terms: ResetTerms<MS>,
    pub opt_remote_reset_terms: Option<ResetTerms<MS>>,
}

#[derive(Clone, Serialize, Deserialize)]
pub enum ChannelStatus<A,P:Clone,RS,FS,MS> {
    Inconsistent(ChannelInconsistent<P,MS>),
    Consistent(TokenChannel<A,P,RS,FS,MS>),
}

impl<A,P,RS,FS,MS> ChannelStatus<A,P,RS,FS,MS> 
where
    A: CanonicalSerialize + Clone,
    P: Eq + Hash + Clone,
    MS: Clone,
{
    pub fn get_last_incoming_move_token_hashed(&self) -> Option<MoveTokenHashed<P,MS>> {
        match &self {
            ChannelStatus::Inconsistent(channel_inconsistent) => 
                channel_inconsistent.opt_last_incoming_move_token.clone(),
            ChannelStatus::Consistent(token_channel) => 
                token_channel.get_last_incoming_move_token_hashed().cloned(),
        }
    }
}

#[allow(unused)]
#[derive(Clone, Serialize, Deserialize)]
pub struct FriendState<A,P:Clone,RS:Clone,FS:Clone,MS> {
    pub local_public_key: TPublicKey<P>,
    pub remote_public_key: TPublicKey<P>,
    pub remote_address: A, 
    pub sent_local_address: SentLocalAddress<A>,
    pub name: String,
    pub channel_status: ChannelStatus<A,P,RS,FS,MS>,
    pub wanted_remote_max_debt: u128,
    pub wanted_local_requests_status: RequestsStatus,
    pub pending_requests: Vector<RequestSendFunds<P>>,
    pub pending_responses: Vector<ResponseOp<P,RS,FS>>,
    // Pending operations to be sent to the token channel.
    pub status: FriendStatus,
    pub pending_user_requests: Vector<RequestSendFunds<P>>,
    // Request that the user has sent to this neighbor, 
    // but have not been processed yet. Bounded in size.
}


#[allow(unused)]
impl<A,P,RS,FS,MS> FriendState<A,P,RS,FS,MS> 
where
    A: CanonicalSerialize + Clone,
    P: Eq + Hash + Clone,
    RS: Clone,
    FS: Clone,
{
    pub fn new(local_public_key: &TPublicKey<P>,
               remote_public_key: &TPublicKey<P>,
               remote_address: A,
               name: String,
               balance: i128) -> Self {

        let token_channel = TokenChannel::new(local_public_key, remote_public_key, balance);

        FriendState {
            local_public_key: local_public_key.clone(),
            remote_public_key: remote_public_key.clone(),
            remote_address,
            sent_local_address: SentLocalAddress::NeverSent,
            name,
            channel_status: ChannelStatus::Consistent(token_channel),

            // The remote_max_debt we want to have. When possible, this will be sent to the remote
            // side.
            wanted_remote_max_debt: 0,
            wanted_local_requests_status: RequestsStatus::Closed,
            // The local_send_price we want to have (Or possibly close requests, by having an empty
            // send price). When possible, this will be updated with the TokenChannel.
            pending_requests: Vector::new(),
            pending_responses: Vector::new(),
            status: FriendStatus::Disabled,
            pending_user_requests: Vector::new(),
        }
    }

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
            ChannelStatus::Consistent(token_channel) =>
                &token_channel.get_mutual_credit().state().balance,
            ChannelStatus::Inconsistent(channel_inconsistent) => return 0,
        };
        balance.local_max_debt.saturating_add_signed(balance.balance)
    }

    #[allow(unused)]
    pub fn mutate(&mut self, friend_mutation: &FriendMutation<A,P,RS,FS,MS>) {
        match friend_mutation {
            FriendMutation::TcMutation(tc_mutation) => {
                match &mut self.channel_status {
                    ChannelStatus::Consistent(ref mut token_channel) =>
                        token_channel.mutate(tc_mutation),
                    ChannelStatus::Inconsistent(_) => unreachable!(),
                }
            },
            FriendMutation::SetInconsistent(channel_inconsistent) => {
                self.channel_status = ChannelStatus::Inconsistent(channel_inconsistent.clone());
            },
            FriendMutation::SetConsistent(token_channel) => {
                self.channel_status = ChannelStatus::Consistent(token_channel.clone());
            },
            FriendMutation::SetWantedRemoteMaxDebt(wanted_remote_max_debt) => {
                self.wanted_remote_max_debt = *wanted_remote_max_debt;
            },
            FriendMutation::SetWantedLocalRequestsStatus(wanted_local_requests_status) => {
                self.wanted_local_requests_status = wanted_local_requests_status.clone();
            },
            FriendMutation::PushBackPendingRequest(request_send_funds) => {
                self.pending_requests.push_back(request_send_funds.clone());
            },
            FriendMutation::PopFrontPendingRequest => {
                let _ = self.pending_requests.pop_front();
            },
            FriendMutation::PushBackPendingResponse(response_op) => {
                self.pending_responses.push_back(response_op.clone());
            },
            FriendMutation::PopFrontPendingResponse => {
                let _ = self.pending_responses.pop_front();
            },
            FriendMutation::PushBackPendingUserRequest(request_send_funds) => {
                self.pending_user_requests.push_back(request_send_funds.clone());
            },
            FriendMutation::PopFrontPendingUserRequest => {
                let _ = self.pending_user_requests.pop_front();
            },
            FriendMutation::SetStatus(friend_status) => {
                self.status = friend_status.clone();
            },
            FriendMutation::SetRemoteAddress(friend_addr) => {
                self.remote_address = friend_addr.clone();
            },
            FriendMutation::SetName(friend_name) => {
                self.name = friend_name.clone();
            },
            FriendMutation::SetSentLocalAddress(sent_local_address) => {
                self.sent_local_address = sent_local_address.clone();
            },
        }
    }
}
