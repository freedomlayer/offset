use std::fmt::Debug;

use crypto::identity::{PublicKey, Signature};
use crypto::crypto_rand::{RandValue, CryptoRandom};

use proto::funder::messages::{RequestSendFunds, FailureSendFunds,
                                PendingFriendRequest};
use proto::funder::signature_buff::{create_failure_signature_buffer};

use crate::handler::MutableFunderHandler;


use crate::types::{ResponseReceived, FunderOutgoingControl,
                    ResponseSendFundsResult,
                    create_pending_request};
use crate::friend::{FriendMutation, ResponseOp, ChannelStatus};
use crate::state::FunderMutation;
use super::sender::SendMode;

use crate::ephemeral::EphemeralMutation;
use crate::freeze_guard::FreezeGuardMutation;


#[allow(unused)]
impl<A: Clone + Debug + 'static, R: CryptoRandom + 'static> MutableFunderHandler<A,R> {

    /// Create a (signed) failure message for a given request_id.
    /// We are the reporting_public_key for this failure message.
    async fn create_failure_message(&self, pending_local_request: PendingFriendRequest) 
        -> FailureSendFunds {

        let rand_nonce = RandValue::new(&self.rng);
        let local_public_key = self.state.local_public_key.clone();

        let mut failure_send_funds = FailureSendFunds {
            request_id: pending_local_request.request_id,
            reporting_public_key: local_public_key,
            rand_nonce,
            signature: Signature::zero(),
        };
        // TODO: Add default() implementation for Signature
        
        let mut failure_signature_buffer = create_failure_signature_buffer(
                                            &failure_send_funds,
                                            &pending_local_request);

        failure_send_funds.signature = await!(self.identity_client.request_signature(failure_signature_buffer))
            .unwrap();

        failure_send_funds
    }

    /// Reply to a request message with failure.
    pub async fn reply_with_failure(&mut self, 
                          remote_public_key: PublicKey,
                          request_send_funds: RequestSendFunds) {

        let pending_request = create_pending_request(&request_send_funds);
        let failure_send_funds = await!(self.create_failure_message(pending_request));

        let failure_op = ResponseOp::Failure(failure_send_funds);
        let friend_mutation = FriendMutation::PushBackPendingResponse(failure_op);
        let funder_mutation = FunderMutation::FriendMutation((remote_public_key.clone(), friend_mutation));
        self.apply_funder_mutation(funder_mutation);
        await!(self.try_send_channel(&remote_public_key, SendMode::EmptyNotAllowed));
    }

    /// Cancel outgoing local requests that are already inside the token channel (Possibly already
    /// communicated to the remote side).
    pub async fn cancel_local_pending_requests(&mut self, 
                                     friend_public_key: PublicKey) {

        let friend = self.get_friend(&friend_public_key).unwrap();

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

        let local_public_key = self.state.local_public_key.clone();
        // Prepare a list of all remote requests that we need to cancel:
        for (local_request_id, pending_local_request) in pending_local_requests {
            let freeze_guard_mutation = FreezeGuardMutation::SubFrozenCredit(
                (pending_local_request.route.clone(), pending_local_request.dest_payment));
            let ephemeral_mutation = EphemeralMutation::FreezeGuardMutation(freeze_guard_mutation);
            self.apply_ephemeral_mutation(ephemeral_mutation);

            let opt_origin_public_key = self.find_request_origin(&local_request_id).cloned();
            let origin_public_key = match opt_origin_public_key {
                Some(origin_public_key) => {
                    // We have found the friend that is the origin of this request.
                    // We send him a failure message.
                    let failure_send_funds = await!(self.create_failure_message(pending_local_request));

                    let failure_op = ResponseOp::Failure(failure_send_funds);
                    let friend_mutation = FriendMutation::PushBackPendingResponse(failure_op);
                    let funder_mutation = FunderMutation::FriendMutation((origin_public_key.clone(), friend_mutation));
                    self.apply_funder_mutation(funder_mutation);
                    await!(self.try_send_channel(&origin_public_key, SendMode::EmptyNotAllowed));
                },
                None => {
                    // We are the origin of this request.
                    // We send a failure response through the control:
                    let response_received = ResponseReceived {
                        request_id: pending_local_request.request_id,
                        result: ResponseSendFundsResult::Failure(self.state.local_public_key.clone()),
                    };
                    self.add_outgoing_control(FunderOutgoingControl::ResponseReceived(response_received));
                },            
            };

        }
    }

    pub async fn cancel_pending_requests(&mut self,
                               friend_public_key: PublicKey) {

        let friend = self.get_friend(&friend_public_key).unwrap();
        let mut pending_requests = friend.pending_requests.clone();

        while let Some(pending_request) = pending_requests.pop_front() {
            let friend_mutation = FriendMutation::PopFrontPendingRequest;
            let funder_mutation = FunderMutation::FriendMutation((friend_public_key.clone(), friend_mutation));
            self.apply_funder_mutation(funder_mutation);

            let opt_origin_public_key = self.find_request_origin(&pending_request.request_id).cloned();
            let origin_public_key = match opt_origin_public_key {
                Some(origin_public_key) => {
                    let pending_request = create_pending_request(&pending_request);
                    let failure_send_funds = await!(self.create_failure_message(pending_request));

                    let failure_op = ResponseOp::Failure(failure_send_funds);
                    let friend_mutation = FriendMutation::PushBackPendingResponse(failure_op);
                    let funder_mutation = FunderMutation::FriendMutation((origin_public_key.clone(), friend_mutation));
                    self.apply_funder_mutation(funder_mutation);
                },
                None => {
                    // We are the origin of this request:
                    let response_received = ResponseReceived {
                        request_id: pending_request.request_id,
                        result: ResponseSendFundsResult::Failure(self.state.local_public_key.clone()),
                    };
                    self.add_outgoing_control(FunderOutgoingControl::ResponseReceived(response_received));
                }, 
            };
        }
    }

    pub async fn cancel_pending_user_requests(&mut self,
                               friend_public_key: PublicKey) {

        let friend = self.get_friend(&friend_public_key).unwrap();
        let mut pending_user_requests = friend.pending_user_requests.clone();

        while let Some(pending_user_request) = pending_user_requests.pop_front() {
            let friend_mutation = FriendMutation::PopFrontPendingUserRequest;
            let funder_mutation = FunderMutation::FriendMutation((friend_public_key.clone(), friend_mutation));
            self.apply_funder_mutation(funder_mutation);

            // We are the origin of this request:
            let response_received = ResponseReceived {
                request_id: pending_user_request.request_id,
                result: ResponseSendFundsResult::Failure(self.state.local_public_key.clone()),
            };
            self.add_outgoing_control(FunderOutgoingControl::ResponseReceived(response_received));
        }
    }
}
