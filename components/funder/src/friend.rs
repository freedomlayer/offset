use im::vector::Vector;

use crypto::identity::PublicKey;
use utils::safe_arithmetic::{SafeSignedArithmetic};

use super::token_channel::TcMutation;
use super::types::{FriendStatus, 
    RequestsStatus, RequestSendFunds, FriendMoveToken,
    ResponseSendFunds, FailureSendFunds,
    ResetTerms, Ratio};
use super::token_channel::TokenChannel;



#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum ResponseOp {
    Response(ResponseSendFunds),
    Failure(FailureSendFunds),
}

#[allow(unused)]
#[derive(Debug)]
pub enum FriendMutation<A> {
    TcMutation(TcMutation),
    SetInconsistent(ChannelInconsistent),
    SetWantedRemoteMaxDebt(u128),
    SetWantedLocalRequestsStatus(RequestsStatus),
    PushBackPendingRequest(RequestSendFunds),
    PopFrontPendingRequest,
    PushBackPendingResponse(ResponseOp),
    PopFrontPendingResponse,
    PushBackPendingUserRequest(RequestSendFunds),
    PopFrontPendingUserRequest,
    SetStatus(FriendStatus),
    SetFriendAddr(A),
    LocalReset(FriendMoveToken),
    // The outgoing move token message we have sent to reset the channel.
    RemoteReset(FriendMoveToken),
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct ChannelInconsistent {
    pub opt_last_incoming_move_token: Option<FriendMoveToken>,
    pub local_reset_terms: ResetTerms,
    pub opt_remote_reset_terms: Option<ResetTerms>,
}

#[derive(Clone, Serialize, Deserialize)]
pub enum ChannelStatus {
    Inconsistent(ChannelInconsistent),
    Consistent(TokenChannel),
}


#[allow(unused)]
#[derive(Clone, Serialize, Deserialize)]
pub struct FriendState<A> {
    pub local_public_key: PublicKey,
    pub remote_public_key: PublicKey,
    pub remote_address: A, 
    pub channel_status: ChannelStatus,
    pub wanted_remote_max_debt: u128,
    pub wanted_local_requests_status: RequestsStatus,
    pub pending_responses: Vector<ResponseOp>,
    pub pending_requests: Vector<RequestSendFunds>,
    // Pending operations to be sent to the token channel.
    pub status: FriendStatus,
    pub pending_user_requests: Vector<RequestSendFunds>,
    // Request that the user has sent to this neighbor, 
    // but have not been processed yet. Bounded in size.
}


#[allow(unused)]
impl<A:Clone + 'static> FriendState<A> {
    pub fn new(local_public_key: &PublicKey,
               remote_public_key: &PublicKey,
               remote_address: A) -> FriendState<A> {
        let token_channel = TokenChannel::new(local_public_key, remote_public_key);
        FriendState {
            local_public_key: local_public_key.clone(),
            remote_public_key: remote_public_key.clone(),
            remote_address,
            channel_status: ChannelStatus::Consistent(token_channel),

            // The remote_max_debt we want to have. When possible, this will be sent to the remote
            // side.
            wanted_remote_max_debt: 0,
            wanted_local_requests_status: RequestsStatus::Closed,
            // The local_send_price we want to have (Or possibly close requests, by having an empty
            // send price). When possible, this will be updated with the TokenChannel.
            pending_requests: Vector::new(),
            pending_responses: Vector::new(),
            status: FriendStatus::Disable,
            pending_user_requests: Vector::new(),
        }
    }

    pub fn shared_credits(&self) -> u128 {
        let balance = match &self.channel_status {
            ChannelStatus::Consistent(token_channel) =>
                token_channel.get_mutual_credit().state().balance.balance,
            ChannelStatus::Inconsistent(channel_inconsistent) =>
                channel_inconsistent.local_reset_terms.balance_for_reset,
        };
        self.wanted_remote_max_debt.checked_sub_signed(balance).unwrap()
    }

    pub fn usable_ratio(&self) -> Ratio<u128> {
        unimplemented!();
    }

    #[allow(unused)]
    pub fn mutate(&mut self, friend_mutation: &FriendMutation<A>) {
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
            FriendMutation::SetFriendAddr(friend_addr) => {
                self.remote_address = friend_addr.clone();
            },
            FriendMutation::LocalReset(reset_move_token) => {
                // Local reset was applied (We sent a reset from the control line)
                match &self.channel_status {
                    ChannelStatus::Consistent(_) => unreachable!(),
                    ChannelStatus::Inconsistent(channel_inconsistent) => {
                        let ChannelInconsistent {
                            opt_last_incoming_move_token,
                            local_reset_terms,
                            opt_remote_reset_terms,
                        } = channel_inconsistent;

                        match opt_remote_reset_terms {
                            None => unreachable!(),
                            Some(remote_reset_terms) => {
                                assert_eq!(reset_move_token.old_token, remote_reset_terms.reset_token);
                                let token_channel = TokenChannel::new_from_local_reset(
                                    &self.local_public_key,
                                    &self.remote_public_key,
                                    &reset_move_token,
                                    remote_reset_terms.balance_for_reset,
                                    opt_last_incoming_move_token.clone());
                                self.channel_status = ChannelStatus::Consistent(token_channel);
                            },
                        }
                    },
                }
            },
            FriendMutation::RemoteReset(reset_move_token) => {
                // Remote reset was applied (Remote side has given a reset command)
                match &self.channel_status {
                    ChannelStatus::Consistent(_) => unreachable!(),
                    ChannelStatus::Inconsistent(channel_inconsistent) => {
                        let token_channel = TokenChannel::new_from_remote_reset(
                            &self.local_public_key,
                            &self.remote_public_key,
                            &reset_move_token,
                            channel_inconsistent.local_reset_terms.balance_for_reset);
                        self.channel_status = ChannelStatus::Consistent(token_channel);
                    },
                }
            },
        }
    }
}
