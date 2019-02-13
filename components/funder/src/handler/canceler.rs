use crypto::identity::PublicKey;

use proto::funder::messages::{RequestSendFunds,
                              ResponseReceived,
                              ResponseSendFundsResult, FunderOutgoingControl};
use proto::funder::scheme::FunderScheme;

use crate::handler::handler::{MutableFunderState, find_request_origin};
use crate::handler::sender::SendCommands;

use crate::types::create_pending_request;
use crate::friend::{FriendMutation, ResponseOp, ChannelStatus};
use crate::state::FunderMutation;


/*
A: CanonicalSerialize + Clone + Debug + PartialEq + Eq + 'static,
R: CryptoRandom + 'static,
*/

    /*
    /// Create a (signed) failure message for a given request_id.
    /// We are the reporting_public_key for this failure message.
    pub fn create_unsigned_failure_message(&self, pending_local_request: &PendingRequest) 
        -> UnsignedFailureSendFunds {

        let rand_nonce = RandValue::new(&self.rng);
        let local_public_key = self.state.local_public_key.clone();

        let mut u_failure_send_funds = UnsignedFailureSendFunds {
            request_id: pending_local_request.request_id.clone(),
            reporting_public_key: local_public_key.clone(),
            rand_nonce,
            signature: (),
        };

        u_failure_send_funds
    }
    */

/// Reply to a request message with failure.
pub fn reply_with_failure<FS>(m_state: &mut MutableFunderState<FS>,
                             send_commands: &mut SendCommands,
                             remote_public_key: &PublicKey,
                             request_send_funds: &RequestSendFunds) 
where
    FS: FunderScheme,
{

    let pending_request = create_pending_request(request_send_funds);
    let u_failure_op = ResponseOp::UnsignedFailure(pending_request);
    let friend_mutation = FriendMutation::PushBackPendingResponse(u_failure_op);
    let funder_mutation = FunderMutation::FriendMutation((remote_public_key.clone(), friend_mutation));
    m_state.mutate(funder_mutation);
    send_commands.set_try_send(remote_public_key);
}

/// Cancel outgoing local requests that are already inside the token channel (Possibly already
/// communicated to the remote side).
pub fn cancel_local_pending_requests<FS>(m_state: &mut MutableFunderState<FS>,
                                     send_commands: &mut SendCommands,
                                     outgoing_control: &mut Vec<FunderOutgoingControl<FS>>,
                                     friend_public_key: &PublicKey) 
where
    FS: FunderScheme,
{


    let friend = m_state.state().friends.get(friend_public_key).unwrap();

    let token_channel = match &friend.channel_status {
        ChannelStatus::Inconsistent(_) => unreachable!(),
        ChannelStatus::Consistent(token_channel) => token_channel,
    };

    // Mark all pending requests to this friend as errors.  
    // As the token channel is being reset, we can be sure we will never obtain a response
    // for those requests.
    let pending_local_requests = token_channel
        .get_mutual_credit()
        .state()
        .pending_requests
        .pending_local_requests
        .clone();

    // Prepare a list of all remote requests that we need to cancel:
    for (local_request_id, pending_local_request) in pending_local_requests {
        let opt_origin_public_key = find_request_origin(m_state.state(), 
                                                        &local_request_id).cloned();
        match opt_origin_public_key {
            Some(origin_public_key) => {
                // We have found the friend that is the origin of this request.
                // We send him a failure message.
                let u_failure_op = ResponseOp::UnsignedFailure(pending_local_request);
                let friend_mutation = FriendMutation::PushBackPendingResponse(u_failure_op);
                let funder_mutation = FunderMutation::FriendMutation((origin_public_key.clone(), friend_mutation));
                m_state.mutate(funder_mutation);
                send_commands.set_try_send(&origin_public_key);
            },
            None => {
                // We are the origin of this request.
                // We send a failure response through the control:
                let response_received = ResponseReceived {
                    request_id: pending_local_request.request_id,
                    result: ResponseSendFundsResult::Failure(m_state.state().local_public_key.clone()),
                };
                outgoing_control.push(FunderOutgoingControl::ResponseReceived(response_received));
            },            
        };
    }
}

pub fn cancel_pending_requests<FS>(m_state: &mut MutableFunderState<FS>,
                                  send_commands: &mut SendCommands,
                                  outgoing_control: &mut Vec<FunderOutgoingControl<FS>>,
                                  friend_public_key: &PublicKey) 
where
    FS: FunderScheme,
{

    let friend = m_state.state().friends.get(friend_public_key).unwrap();
    let mut pending_requests = friend.pending_requests.clone();

    while let Some(pending_request) = pending_requests.pop_front() {
        let friend_mutation = FriendMutation::PopFrontPendingRequest;
        let funder_mutation = FunderMutation::FriendMutation((friend_public_key.clone(), friend_mutation));
        m_state.mutate(funder_mutation);

        let opt_origin_public_key = find_request_origin(m_state.state(), 
                                                        &pending_request.request_id).cloned();
        match opt_origin_public_key {
            Some(origin_public_key) => {
                let local_pending_request = create_pending_request(&pending_request);
                let u_failure_op = ResponseOp::UnsignedFailure(local_pending_request);
                let friend_mutation = FriendMutation::PushBackPendingResponse(u_failure_op);
                let funder_mutation = FunderMutation::FriendMutation((origin_public_key.clone(), friend_mutation));
                m_state.mutate(funder_mutation);
                send_commands.set_try_send(&origin_public_key);
            },
            None => {
                // We are the origin of this request:
                let response_received = ResponseReceived {
                    request_id: pending_request.request_id,
                    result: ResponseSendFundsResult::Failure(m_state.state().local_public_key.clone()),
                };
                outgoing_control.push(FunderOutgoingControl::ResponseReceived(response_received));
            }, 
        };
    }
}

pub fn cancel_pending_user_requests<FS>(m_state: &mut MutableFunderState<FS>,
                                       outgoing_control: &mut Vec<FunderOutgoingControl<FS>>,
                                       friend_public_key: &PublicKey) 
where
    FS: FunderScheme,
{

    let friend = m_state.state().friends.get(&friend_public_key).unwrap();
    let mut pending_user_requests = friend.pending_user_requests.clone();

    while let Some(pending_user_request) = pending_user_requests.pop_front() {
        let friend_mutation = FriendMutation::PopFrontPendingUserRequest;
        let funder_mutation = FunderMutation::FriendMutation((friend_public_key.clone(), friend_mutation));
        m_state.mutate(funder_mutation);

        // We are the origin of this request:
        let response_received = ResponseReceived {
            request_id: pending_user_request.request_id,
            result: ResponseSendFundsResult::Failure(m_state.state().local_public_key.clone()),
        };
        outgoing_control.push(FunderOutgoingControl::ResponseReceived(response_received));
    }
}
