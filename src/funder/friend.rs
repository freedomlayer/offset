use im::vector::Vector;

use crypto::identity::PublicKey;
use crypto::uid::Uid;

use super::token_channel::directional::{DirectionalMutation, MoveTokenDirection};
use proto::funder::ChannelToken;
use super::types::{FriendTcOp, FriendStatus, 
    RequestsStatus, RequestSendFunds, FriendMoveToken,
    ResponseSendFunds, FailureSendFunds, UserRequestSendFunds};
use super::token_channel::directional::DirectionalTokenChannel;



#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResetTerms {
    pub current_token: ChannelToken,
    pub balance_for_reset: i128,
}

#[derive(Clone)]
pub enum InconsistencyStatus {
    Empty,
    Outgoing(ResetTerms),
    IncomingOutgoing((ResetTerms, ResetTerms)), 
    // (incoming_reset_terms, outgoing_reset_terms)
}

impl InconsistencyStatus {
    pub fn new() -> InconsistencyStatus {
        InconsistencyStatus::Empty
    }
}

#[derive(Clone)]
pub enum ResponseOp {
    Response(ResponseSendFunds),
    Failure(FailureSendFunds),
}

#[allow(unused)]
pub enum FriendMutation<A> {
    DirectionalMutation(DirectionalMutation),
    SetInconsistencyStatus(InconsistencyStatus),
    SetWantedRemoteMaxDebt(u128),
    SetWantedLocalRequestsStatus(RequestsStatus),
    PushBackPendingRequest(RequestSendFunds),
    PopFrontPendingRequest,
    PushBackPendingResponse(ResponseOp),
    PopFrontPendingResponse,
    PushBackPendingUserRequest(UserRequestSendFunds),
    PopFrontPendingUserRequest,
    SetStatus(FriendStatus),
    SetFriendAddr(Option<A>),
    LocalReset(FriendMoveToken),
    // The outgoing move token message we have sent to reset the channel.
    RemoteReset,
}

#[allow(unused)]
#[derive(Clone)]
pub struct FriendState<A> {
    pub opt_remote_address: Option<A>, 
    pub directional: DirectionalTokenChannel,
    pub inconsistency_status: InconsistencyStatus,
    pub wanted_remote_max_debt: u128,
    pub wanted_local_requests_status: RequestsStatus,
    pub pending_responses: Vector<ResponseOp>,
    pub pending_requests: Vector<RequestSendFunds>,
    // Pending operations to be sent to the token channel.
    pub status: FriendStatus,
    pub pending_user_requests: Vector<UserRequestSendFunds>,
    // Request that the user has sent to this neighbor, 
    // but have not been processed yet. Bounded in size.
}


#[allow(unused)]
impl<A:Clone> FriendState<A> {
    pub fn new(local_public_key: &PublicKey,
               remote_public_key: &PublicKey,
               opt_remote_address: Option<A>) -> FriendState<A> {
        FriendState {
            opt_remote_address,
            directional: DirectionalTokenChannel::new(local_public_key,
                                           remote_public_key),

            inconsistency_status: InconsistencyStatus::new(),
            // The remote_max_debt we want to have. When possible, this will be sent to the remote
            // side.
            wanted_remote_max_debt: 0,
            wanted_local_requests_status: RequestsStatus::Closed,
            // The local_send_price we want to have (Or possibly close requests, by having an empty
            // send price). When possible, this will be updated with the TokenChannel.
            pending_requests: Vector::new(),
            pending_responses: Vector::new(),
            status: FriendStatus::Enable,
            pending_user_requests: Vector::new(),
        }
    }

    #[allow(unused)]
    pub fn mutate(&mut self, friend_mutation: &FriendMutation<A>) {
        match friend_mutation {
            FriendMutation::DirectionalMutation(directional_mutation) => {
                self.directional.mutate(directional_mutation);
            },
            FriendMutation::SetInconsistencyStatus(inconsistency_status) => {
                self.inconsistency_status = inconsistency_status.clone();
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
            FriendMutation::PushBackPendingUserRequest(user_request_send_funds) => {
                self.pending_user_requests.push_back(user_request_send_funds.clone());
            },
            FriendMutation::PopFrontPendingUserRequest => {
                let _ = self.pending_user_requests.pop_front();
            },
            FriendMutation::SetStatus(friend_status) => {
                self.status = friend_status.clone();
            },
            FriendMutation::SetFriendAddr(opt_friend_addr) => {
                self.opt_remote_address = opt_friend_addr.clone();
            },
            FriendMutation::LocalReset(reset_move_token) => {
                // Local reset was applied (We sent a reset from the control line)
                match &self.inconsistency_status {
                    InconsistencyStatus::Empty => unreachable!(),
                    InconsistencyStatus::Outgoing(_) => unreachable!(),
                    InconsistencyStatus::IncomingOutgoing((in_reset_terms, _)) => {
                        assert_eq!(reset_move_token.old_token, in_reset_terms.current_token);
                        self.directional = DirectionalTokenChannel::new_from_local_reset(
                            &self.directional.token_channel.state().idents.local_public_key,
                            &self.directional.token_channel.state().idents.remote_public_key,
                            &reset_move_token,
                            in_reset_terms.balance_for_reset);
                    }
                };
                self.inconsistency_status = InconsistencyStatus::new();
            },
            FriendMutation::RemoteReset => {
                // Remote reset was applied (Remote side has given a reset command)
                let reset_token = self.directional.calc_channel_reset_token();
                let balance_for_reset = self.directional.balance_for_reset();
                self.directional = DirectionalTokenChannel::new_from_remote_reset(
                    &self.directional.token_channel.state().idents.local_public_key,
                    &self.directional.token_channel.state().idents.remote_public_key,
                    &reset_token,
                    balance_for_reset);
                self.inconsistency_status = InconsistencyStatus::new();
            },
        }
    }
}
