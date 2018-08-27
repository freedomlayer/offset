use futures::prelude::{async, await};
use ring::rand::SecureRandom;

use crypto::identity::{PublicKey, Signature};
use crypto::rand_values::RandValue;

use super::{MutableFunderHandler, FunderTask, FriendMessage,
            ResponseReceived};

use super::super::types::{RequestSendFunds, FailureSendFunds, PendingFriendRequest};
use super::super::signature_buff::{create_failure_signature_buffer, prepare_receipt};
use super::super::friend::{FriendMutation, ResponseOp};
use super::super::state::FunderMutation;
use super::super::messages::{ResponseSendFundsResult};


#[allow(unused)]
impl<A: Clone + 'static, R: SecureRandom + 'static> MutableFunderHandler<A,R> {

    /// Create a (signed) failure message for a given request_id.
    /// We are the reporting_public_key for this failure message.
    #[async]
    fn create_failure_message(self, pending_local_request: PendingFriendRequest) 
        -> Result<(Self, FailureSendFunds), !> {

        let rand_nonce = RandValue::new(&*self.rng);
        let local_public_key = self.state.get_local_public_key().clone();

        let failure_send_funds = FailureSendFunds {
            request_id: pending_local_request.request_id,
            reporting_public_key: local_public_key.clone(),
            rand_nonce: rand_nonce.clone(),
            signature: Signature::zero(),
        };
        // TODO: Add default() implementation for Signature
        
        let mut failure_signature_buffer = create_failure_signature_buffer(
                                            &failure_send_funds,
                                            &pending_local_request);

        let signature = await!(self.security_module_client.request_signature(failure_signature_buffer))
            .unwrap();

        Ok((self, FailureSendFunds {
            request_id: pending_local_request.request_id,
            reporting_public_key: local_public_key,
            rand_nonce,
            signature,
        }))
    }

    /// Reply to a request message with failure.
    #[async]
    pub fn reply_with_failure(self, 
                          remote_public_key: PublicKey,
                          request_send_funds: RequestSendFunds) -> Result<Self, !> {

        let pending_request = request_send_funds.create_pending_request();
        let (mut fself, failure_send_funds) = await!(self.create_failure_message(pending_request))?;

        let failure_op = ResponseOp::Failure(failure_send_funds);
        let friend_mutation = FriendMutation::PushBackPendingResponse(failure_op);
        let messenger_mutation = FunderMutation::FriendMutation((remote_public_key.clone(), friend_mutation));
        fself.apply_mutation(messenger_mutation);
        fself.try_send_channel(&remote_public_key);

        Ok(fself)
    }

    #[async]
    pub fn cancel_local_pending_requests(mut self, 
                                     friend_public_key: PublicKey) -> Result<Self, !> {

        let friend = self.get_friend(&friend_public_key).unwrap();

        // Mark all pending requests to this friend as errors.  
        // As the token channel is being reset, we can be sure we will never obtain a response
        // for those requests.
        let pending_local_requests = friend.directional
            .token_channel
            .state()
            .pending_requests
            .pending_local_requests
            .clone();

        let local_public_key = self.state.get_local_public_key().clone();
        let mut fself = self;
        // Prepare a list of all remote requests that we need to cancel:
        for (local_request_id, pending_local_request) in pending_local_requests {
            let opt_origin_public_key = fself.find_request_origin(&local_request_id).cloned();
            let origin_public_key = match opt_origin_public_key {
                Some(origin_public_key) => {
                    // We have found the friend that is the origin of this request.
                    // We send him a failure message.
                    let (new_fself, failure_send_funds) = await!(fself.create_failure_message(pending_local_request))?;
                    fself = new_fself;

                    let failure_op = ResponseOp::Failure(failure_send_funds);
                    let friend_mutation = FriendMutation::PushBackPendingResponse(failure_op);
                    let messenger_mutation = FunderMutation::FriendMutation((origin_public_key.clone(), friend_mutation));
                    fself.apply_mutation(messenger_mutation);
                    fself.try_send_channel(&origin_public_key);
                },
                None => {
                    // We are the origin of this request.
                    // We send a failure response through the control:
                    let response_received = ResponseReceived {
                        request_id: pending_local_request.request_id,
                        result: ResponseSendFundsResult::Failure(fself.state.local_public_key.clone()),
                    };
                    fself.funder_tasks.push(FunderTask::ResponseReceived(response_received));
                },            
            };

        }
        Ok(fself)
    }

    #[async]
    pub fn cancel_pending_requests(mut self,
                               friend_public_key: PublicKey)
                        -> Result<Self, !> {

        let friend = self.get_friend(&friend_public_key).unwrap();
        let mut pending_requests = friend.pending_requests.clone();
        let mut fself = self;

        while let Some(pending_request) = pending_requests.pop_front() {
            let friend_mutation = FriendMutation::PopFrontPendingRequest;
            let messenger_mutation = FunderMutation::FriendMutation((friend_public_key.clone(), friend_mutation));
            fself.apply_mutation(messenger_mutation);

            let opt_origin_public_key = fself.find_request_origin(&pending_request.request_id).cloned();
            let origin_public_key = match opt_origin_public_key {
                Some(origin_public_key) => {
                    let pending_request = pending_request.create_pending_request();
                    let (new_fself, failure_send_funds) = await!(fself.create_failure_message(pending_request)).unwrap();
                    fself = new_fself;

                    let failure_op = ResponseOp::Failure(failure_send_funds);
                    let friend_mutation = FriendMutation::PushBackPendingResponse(failure_op);
                    let messenger_mutation = FunderMutation::FriendMutation((origin_public_key.clone(), friend_mutation));
                    fself.apply_mutation(messenger_mutation);
                },
                None => {
                    // We are the origin of this request:
                    let response_received = ResponseReceived {
                        request_id: pending_request.request_id,
                        result: ResponseSendFundsResult::Failure(fself.state.local_public_key.clone()),
                    };
                    fself.funder_tasks.push(FunderTask::ResponseReceived(response_received));
                }, 
            };
        }
        Ok(fself)
    }

    #[async]
    pub fn cancel_pending_user_requests(mut self,
                               friend_public_key: PublicKey)
                        -> Result<Self, !> {

        let friend = self.get_friend(&friend_public_key).unwrap();
        let mut pending_user_requests = friend.pending_user_requests.clone();
        let mut fself = self;

        while let Some(pending_user_request) = pending_user_requests.pop_front() {
            let friend_mutation = FriendMutation::PopFrontPendingUserRequest;
            let messenger_mutation = FunderMutation::FriendMutation((friend_public_key.clone(), friend_mutation));
            fself.apply_mutation(messenger_mutation);

            // We are the origin of this request:
            let response_received = ResponseReceived {
                request_id: pending_user_request.request_id,
                result: ResponseSendFundsResult::Failure(fself.state.local_public_key.clone()),
            };
            fself.funder_tasks.push(FunderTask::ResponseReceived(response_received));
        }
        Ok(fself)
    }
}
