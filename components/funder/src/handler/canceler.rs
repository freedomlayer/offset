use crypto::identity::{PublicKey, Signature};
use crypto::crypto_rand::{RandValue, CryptoRandom};

use super::{MutableFunderHandler};

use super::super::types::{RequestSendFunds, FailureSendFunds, PendingFriendRequest,
                            ResponseReceived};
use super::super::signature_buff::{create_failure_signature_buffer};
use super::super::friend::{FriendMutation, ResponseOp, ChannelStatus};
use super::super::state::FunderMutation;
use super::super::messages::{ResponseSendFundsResult};
use super::sender::SendMode;


#[allow(unused)]
impl<A: Clone + 'static, R: CryptoRandom + 'static> MutableFunderHandler<A,R> {

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

        let pending_request = request_send_funds.create_pending_request();
        let failure_send_funds = await!(self.create_failure_message(pending_request));

        let failure_op = ResponseOp::Failure(failure_send_funds);
        let friend_mutation = FriendMutation::PushBackPendingResponse(failure_op);
        let messenger_mutation = FunderMutation::FriendMutation((remote_public_key.clone(), friend_mutation));
        self.apply_mutation(messenger_mutation);
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
            self.ephemeral.freeze_guard.sub_frozen_credit(
                &pending_local_request.route, pending_local_request.dest_payment);

            let opt_origin_public_key = self.find_request_origin(&local_request_id).cloned();
            let origin_public_key = match opt_origin_public_key {
                Some(origin_public_key) => {
                    // We have found the friend that is the origin of this request.
                    // We send him a failure message.
                    let failure_send_funds = await!(self.create_failure_message(pending_local_request));

                    let failure_op = ResponseOp::Failure(failure_send_funds);
                    let friend_mutation = FriendMutation::PushBackPendingResponse(failure_op);
                    let messenger_mutation = FunderMutation::FriendMutation((origin_public_key.clone(), friend_mutation));
                    self.apply_mutation(messenger_mutation);
                    await!(self.try_send_channel(&origin_public_key, SendMode::EmptyNotAllowed));
                },
                None => {
                    // We are the origin of this request.
                    // We send a failure response through the control:
                    let response_received = ResponseReceived {
                        request_id: pending_local_request.request_id,
                        result: ResponseSendFundsResult::Failure(self.state.local_public_key.clone()),
                    };
                    self.add_response_received(response_received);
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
            let messenger_mutation = FunderMutation::FriendMutation((friend_public_key.clone(), friend_mutation));
            self.apply_mutation(messenger_mutation);

            let opt_origin_public_key = self.find_request_origin(&pending_request.request_id).cloned();
            let origin_public_key = match opt_origin_public_key {
                Some(origin_public_key) => {
                    let pending_request = pending_request.create_pending_request();
                    let failure_send_funds = await!(self.create_failure_message(pending_request));

                    let failure_op = ResponseOp::Failure(failure_send_funds);
                    let friend_mutation = FriendMutation::PushBackPendingResponse(failure_op);
                    let messenger_mutation = FunderMutation::FriendMutation((origin_public_key.clone(), friend_mutation));
                    self.apply_mutation(messenger_mutation);
                },
                None => {
                    // We are the origin of this request:
                    let response_received = ResponseReceived {
                        request_id: pending_request.request_id,
                        result: ResponseSendFundsResult::Failure(self.state.local_public_key.clone()),
                    };
                    self.add_response_received(response_received);
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
            let messenger_mutation = FunderMutation::FriendMutation((friend_public_key.clone(), friend_mutation));
            self.apply_mutation(messenger_mutation);

            // We are the origin of this request:
            let response_received = ResponseReceived {
                request_id: pending_user_request.request_id,
                result: ResponseSendFundsResult::Failure(self.state.local_public_key.clone()),
            };
            self.add_response_received(response_received);
        }
    }
}
