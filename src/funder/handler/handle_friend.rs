use std::convert::TryFrom;

use futures::prelude::{async, await};

use num_bigint::BigUint;
use num_traits::ToPrimitive;

use ring::rand::SecureRandom;

use crypto::rand_values::RandValue;
use crypto::identity::{PublicKey, Signature};
use crypto::uid::Uid;

use utils::safe_arithmetic::SafeArithmetic;


use proto::funder::ChannelToken;

use super::super::token_channel::incoming::{IncomingResponseSendFunds, 
    IncomingFailureSendFunds, IncomingMessage};
use super::super::token_channel::outgoing::{OutgoingTokenChannel, QueueOperationFailure,
    QueueOperationError};
use super::super::token_channel::directional::{ReceiveMoveTokenOutput, ReceiveMoveTokenError, 
    DirectionalMutation, MoveTokenDirection, MoveTokenReceived};
use super::{MutableFunderHandler, FunderTask, FriendMessage,
            ResponseReceived};
use super::super::types::{FriendTcOp, RequestSendFunds, 
    ResponseSendFunds, FailureSendFunds, 
    FriendMoveToken};
use super::super::token_channel::types::FriendMoveTokenInner;
use super::super::state::FunderMutation;
use super::super::friend::{FriendState, FriendMutation, OutgoingInconsistency, IncomingInconsistency, ResetTerms};

use super::super::signature_buff::{create_failure_signature_buffer, prepare_receipt};
use super::super::types::{FunderFreezeLink, PkPairPosition, PendingFriendRequest, Ratio};
use super::super::messages::{RequestsStatus, ResponseSendFundsMsg};

use proto::common::SendFundsReceipt;

// Approximate maximum size of a MOVE_TOKEN message.
// TODO: Where to put this constant? Do we have more like this one?
const MAX_MOVE_TOKEN_LENGTH: usize = 0x1000;


#[allow(unused)]
pub struct FriendInconsistencyError {
    opt_ack: Option<ChannelToken>,
    current_token: ChannelToken,
    balance_for_reset: i128,
}


#[allow(unused)]
pub enum IncomingFriendMessage {
    MoveToken(FriendMoveToken),
    InconsistencyError(FriendInconsistencyError),
}

pub enum HandleFriendError {
}


#[allow(unused)]
impl<A: Clone + 'static, R: SecureRandom + 'static> MutableFunderHandler<A,R> {

    fn get_friend(&self, friend_public_key: &PublicKey) -> Option<&FriendState<A>> {
        self.state.get_friends().get(&friend_public_key)
    }

    /// Find the originator of a pending local request.
    /// This should be a pending remote request at some other friend.
    /// Returns the public key of a friend. If we are the origin of this request, the function return None.
    ///
    /// TODO: We need to change this search to be O(1) in the future. Possibly by maintaining a map
    /// between request_id and (friend_public_key, friend).
    fn find_request_origin(&self, request_id: &Uid) -> Option<&PublicKey> {
        for (friend_public_key, friend) in self.state.get_friends() {
            if friend.directional
                .token_channel
                .state()
                .pending_requests
                .pending_remote_requests
                .contains_key(request_id) {
                    return Some(friend_public_key)
            }
        }
        None
    }

    /// Create a (signed) failure message for a given request_id.
    /// We are the reporting_public_key for this failure message.
    #[async]
    fn create_failure_message(mut self, pending_local_request: PendingFriendRequest) 
        -> Result<(Self, FailureSendFunds), HandleFriendError> {

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


    #[async]
    fn cancel_local_pending_requests(mut self, 
                                     friend_public_key: PublicKey) -> Result<Self, HandleFriendError> {

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
            let opt_origin_public_key = fself.find_request_origin(&local_request_id);
            let origin_public_key = match opt_origin_public_key {
                Some(origin_public_key) => origin_public_key.clone(),
                None => continue,
            };

            let (new_fself, failure_send_funds) = await!(fself.create_failure_message(pending_local_request))?;
            fself = new_fself;

            let failure_op = FriendTcOp::FailureSendFunds(failure_send_funds);
            let friend_mutation = FriendMutation::PushBackPendingOperation(failure_op);
            let messenger_mutation = FunderMutation::FriendMutation((origin_public_key.clone(), friend_mutation));
            fself.apply_mutation(messenger_mutation);
        }
        Ok(fself)
   }


    /// Check if channel reset is required (Remove side used the RESET token)
    /// If so, reset the channel.
    #[async]
    fn check_reset_channel(mut self, 
                           friend_public_key: PublicKey,
                           new_token: ChannelToken) -> Result<Self, HandleFriendError> {
        // Check if incoming message is an attempt to reset channel.
        // We can know this by checking if new_token is a special value.
        let friend = self.get_friend(&friend_public_key).unwrap();
        let reset_token = friend.directional.calc_channel_reset_token();
        let balance_for_reset = friend.directional.balance_for_reset();

        if new_token == reset_token {
            // This is a reset message. We reset the token channel:
            let mut fself = await!(self.cancel_local_pending_requests(
                friend_public_key.clone()))?;

            let friend_mutation = FriendMutation::RemoteReset;
            let messenger_mutation = FunderMutation::FriendMutation((friend_public_key.clone(), friend_mutation));
            fself.apply_mutation(messenger_mutation);

            Ok(fself)
        } else {
            Ok(self)
        }
    }


    /// Reply to a request message with failure.
    #[async]
    fn reply_with_failure(self, 
                          remote_public_key: PublicKey,
                          request_send_funds: RequestSendFunds) -> Result<Self, HandleFriendError> {

        let pending_request = request_send_funds.create_pending_request();
        let (mut fself, failure_send_funds) = await!(self.create_failure_message(pending_request))?;

        let failure_op = FriendTcOp::FailureSendFunds(failure_send_funds);
        let friend_mutation = FriendMutation::PushBackPendingOperation(failure_op);
        let messenger_mutation = FunderMutation::FriendMutation((remote_public_key.clone(), friend_mutation));
        fself.apply_mutation(messenger_mutation);

        Ok(fself)
    }

    /// Forward a request message to the relevant friend and token channel.
    fn forward_request(&mut self, mut request_send_funds: RequestSendFunds) {
        let index = request_send_funds.route.pk_to_index(self.state.get_local_public_key())
            .unwrap();
        let prev_index = index.checked_sub(1).unwrap();
        let next_index = index.checked_add(1).unwrap();
        
        let prev_pk = request_send_funds.route.index_to_pk(prev_index).unwrap();
        let next_pk = request_send_funds.route.index_to_pk(prev_index).unwrap();

        let prev_friend = self.state.get_friends().get(&prev_pk).unwrap();
        let next_friend = self.state.get_friends().get(&next_pk).unwrap();


        let total_trust = self.state.get_total_trust();

        let prev_trust: BigUint = prev_friend.directional.token_channel.state().balance.remote_max_debt.into();
        let forward_trust: BigUint = next_friend.directional.token_channel.state().balance.remote_max_debt.into();

        let two_pow_128 = BigUint::new(vec![0x1, 0x0u32, 0x0u32, 0x0u32, 0x0u32]);
        let numerator = (two_pow_128 * forward_trust) / (total_trust - &prev_trust);
        let usable_ratio = match numerator.to_u128() {
            Some(num) => Ratio::Numerator(num),
            None => Ratio::One,
        };

        let shared_credits = prev_trust.to_u128().unwrap_or(u128::max_value());

        // Add our freeze link
        request_send_funds.freeze_links.push(FunderFreezeLink {
            shared_credits,
            usable_ratio,
        });

        // Queue message to the relevant friend. Later this message will be queued to a specific
        // available token channel:
        let request_op = FriendTcOp::RequestSendFunds(request_send_funds.clone());
        let friend_mutation = FriendMutation::PushBackPendingOperation(request_op);
        let messenger_mutation = FunderMutation::FriendMutation((next_pk.clone(), friend_mutation));
        self.apply_mutation(messenger_mutation);
    }

    #[async]
    fn handle_request_send_funds(mut self, 
                               remote_public_key: PublicKey,
                               request_send_funds: RequestSendFunds) -> Result<Self, HandleFriendError> {

        self.cache.freeze_guard.add_frozen_credit(&request_send_funds.create_pending_request());
        // TODO: Add rest of add/sub_frozen_credit

        // Find ourselves on the route. If we are not there, abort.
        let remote_index = request_send_funds.route.find_pk_pair(
            &remote_public_key, 
            self.state.get_local_public_key()).unwrap();

        let local_index = remote_index.checked_add(1).unwrap();
        let next_index = local_index.checked_add(1).unwrap();
        if next_index >= request_send_funds.route.len() {
            // self.punt_request_to_crypter(request_send_funds);
            return Ok(self);
        }


        // The node on the route has to be one of our friends:
        let next_public_key = request_send_funds.route.index_to_pk(next_index).unwrap();
        let mut fself = if !self.state.get_friends().contains_key(next_public_key) {
            await!(self.reply_with_failure(remote_public_key.clone(), 
                                           request_send_funds.clone()))?
        } else {
            self
        };


        // Perform DoS protection check:
        Ok(match fself.cache.freeze_guard.verify_freezing_links(&request_send_funds) {
            Some(()) => {
                // Add our freezing link, and queue message to the next node.
                fself.forward_request(request_send_funds);
                fself
            },
            None => {
                // Queue a failure message to this token channel:
                await!(fself.reply_with_failure(remote_public_key, 
                                               request_send_funds))?
            },
        })
    }


    fn handle_response_send_funds(&mut self, 
                               remote_public_key: &PublicKey,
                               response_send_funds: ResponseSendFunds,
                               pending_request: PendingFriendRequest) {

        self.cache.freeze_guard.sub_frozen_credit(&pending_request);
        match self.find_request_origin(&response_send_funds.request_id) {
            None => {
                // We are the origin of this request, and we got a response.
                // We should pass it back to crypter.


                let receipt = prepare_receipt(&response_send_funds,
                                              &pending_request);

                let response_send_funds_msg = ResponseSendFundsMsg::Success(receipt);
                self.funder_tasks.push(
                    FunderTask::ResponseReceived(
                        ResponseReceived {
                            request_id: pending_request.request_id,
                            response_send_funds_msg,
                        }
                    )
                );
            },
            Some(friend_public_key) => {
                // Queue this response message to another token channel:
                let response_op = FriendTcOp::ResponseSendFunds(response_send_funds);
                let friend_mutation = FriendMutation::PushBackPendingOperation(response_op);
                let messenger_mutation = FunderMutation::FriendMutation((friend_public_key.clone(), friend_mutation));
                self.apply_mutation(messenger_mutation);
            },
        }
    }

    #[async]
    fn handle_failure_send_funds(mut self, 
                               remote_public_key: &PublicKey,
                               failure_send_funds: FailureSendFunds,
                               pending_request: PendingFriendRequest)
                                -> Result<Self, HandleFriendError> {

        self.cache.freeze_guard.sub_frozen_credit(&pending_request);
        let fself = match self.find_request_origin(&failure_send_funds.request_id) {
            None => {
                // We are the origin of this request, and we got a failure
                // We should pass it back to crypter.


                let response_send_funds_msg = ResponseSendFundsMsg::Failure(failure_send_funds.reporting_public_key);
                self.funder_tasks.push(
                    FunderTask::ResponseReceived(
                        ResponseReceived {
                            request_id: pending_request.request_id,
                            response_send_funds_msg,
                        }
                    )
                );

                self
            },
            Some(friend_public_key) => {
                // Queue this failure message to another token channel:
                let failure_op = FriendTcOp::FailureSendFunds(failure_send_funds);
                let friend_mutation = FriendMutation::PushBackPendingOperation(failure_op);
                let messenger_mutation = FunderMutation::FriendMutation((friend_public_key.clone(), friend_mutation));
                self.apply_mutation(messenger_mutation);

                self
            },
        };
        Ok(fself)
    }

    /// Process valid incoming operations from remote side.
    #[async]
    fn handle_move_token_output(mut self, 
                                remote_public_key: PublicKey,
                                incoming_messages: Vec<IncomingMessage> )
                        -> Result<Self, HandleFriendError> {

        let mut fself = self;
        for incoming_message in incoming_messages {
            fself = match incoming_message {
                IncomingMessage::Request(request_send_funds) => 
                    await!(fself.handle_request_send_funds(remote_public_key.clone(),
                                                 request_send_funds))?,
                IncomingMessage::Response(IncomingResponseSendFunds {
                                                pending_request, incoming_response}) => {
                    fself.handle_response_send_funds(&remote_public_key, 
                                                  incoming_response, pending_request);
                    fself
                },
                IncomingMessage::Failure(IncomingFailureSendFunds {
                                                pending_request, incoming_failure}) => {
                    await!(fself.handle_failure_send_funds(&remote_public_key, 
                                                 incoming_failure, pending_request))?
                },
            }
        }
        Ok(fself)
    }

    /// Handle an error with incoming move token.
    fn handle_move_token_error(&mut self,
                               remote_public_key: &PublicKey,
                               receive_move_token_error: ReceiveMoveTokenError) {

        // Clear current incoming inconsistency messages:
        let friend_mutation = FriendMutation::SetIncomingInconsistency(IncomingInconsistency::Empty);
        let messenger_mutation = FunderMutation::FriendMutation((remote_public_key.clone(), friend_mutation));
        self.apply_mutation(messenger_mutation);


        let friend = self.get_friend(&remote_public_key).unwrap();
        // Send an InconsistencyError message to remote side:
        let current_token = friend.directional.calc_channel_reset_token();
        let balance_for_reset = friend.directional
            .balance_for_reset();

        let inconsistency_error = FriendInconsistencyError {
            opt_ack: None,
            current_token,
            balance_for_reset,
        };

        self.funder_tasks.push(
            FunderTask::FriendMessage(
                FriendMessage::InconsistencyError(inconsistency_error)));

        // Keep outgoing InconsistencyError message details in memory:
        let friend_mutation = FriendMutation::SetOutgoingInconsistency(OutgoingInconsistency::Sent);
        let messenger_mutation = FunderMutation::FriendMutation((remote_public_key.clone(), friend_mutation));
        self.apply_mutation(messenger_mutation);
    }


    /// Queue as many messages as possible into available token channel.
    fn queue_outgoing_operations(&mut self,
                           remote_public_key: &PublicKey,
                           out_tc: &mut OutgoingTokenChannel) -> Result<(), QueueOperationFailure> {

        let friend = self.get_friend(remote_public_key).unwrap();

        // Set remote_max_debt if needed:
        let remote_max_debt = friend
            .directional
            .remote_max_debt();

        if friend.wanted_remote_max_debt != remote_max_debt {
            out_tc.queue_operation(FriendTcOp::SetRemoteMaxDebt(friend.wanted_remote_max_debt))?;
        }

        // Open or close requests is needed:
        unimplemented!();


        let local_requests_status = &friend
            .directional
            .token_channel
            .state()
            .requests_status
            .local;

        if friend.wanted_local_requests_status != *local_requests_status {
            let friend_op = if let RequestsStatus::Open = friend.wanted_local_requests_status {
                FriendTcOp::EnableRequests
            } else {
                FriendTcOp::DisableRequests
            };
            out_tc.queue_operation(friend_op)?;
        }

        // Send pending operations (responses and failures)
        // TODO: Possibly replace this clone with something more efficient later:
        let mut pending_operations = friend.pending_operations.clone();
        while let Some(pending_operation) = pending_operations.pop_front() {
            out_tc.queue_operation(pending_operation)?;
            let friend_mutation = FriendMutation::PopFrontPendingOperation;
            let messenger_mutation = FunderMutation::FriendMutation((remote_public_key.clone(), friend_mutation));
            self.apply_mutation(messenger_mutation);
        }

        Ok(())
    }

    /// Compose a large as possible message to send through the token channel to the remote side.
    /// The message should contain various operations, collected from:
    /// - Generic pending requests (Might be sent through any token channel).
    /// - Token channel specific pending responses/failures.
    /// - Commands that were initialized through AppManager.
    ///
    /// Any operations that will enter the message should be applied. For example, a failure
    /// message should cause the pending request to be removed.
    ///
    /// received_empty -- is the move token message we have just received empty?
    fn send_through_token_channel(&mut self, 
                                  remote_public_key: &PublicKey,
                                  received_empty: bool) {

        let friend = self.get_friend(remote_public_key).unwrap();
        let mut out_tc = friend.directional
            .begin_outgoing_move_token(MAX_MOVE_TOKEN_LENGTH).unwrap();

        let res = self.queue_outgoing_operations(remote_public_key, &mut out_tc);

        // If we had a real error, we should panic.
        // Otherwise
        match res {
            Ok(()) => {},
            Err(QueueOperationFailure {error, ..}) => {
                match error {
                    QueueOperationError::MaxLengthReached => {},
                    _ => unreachable!(),
                }
            }
        };

        let (operations, tc_mutations) = out_tc.done();

        // If there is nothing to send, and the transaction we have received is nonempty, send an empty message back as ack.
        //
        // If the message received is empty and there is nothing to send, we do nothing. (There
        // is no reason to send an ack for an empty message).

        // If we received an empty move token message, and we have nothing to send, 
        // we do nothing:
        if received_empty && operations.is_empty() {
            return;
        }

        for tc_mutation in tc_mutations {
            let directional_mutation = DirectionalMutation::TcMutation(tc_mutation);
            let friend_mutation = FriendMutation::DirectionalMutation(directional_mutation);
            let messenger_mutation = FunderMutation::FriendMutation((remote_public_key.clone(), friend_mutation));
            self.apply_mutation(messenger_mutation);
        }
        let friend = self.get_friend(remote_public_key).unwrap();

        let rand_nonce = RandValue::new(&*self.rng);
        let friend_move_token_inner = FriendMoveTokenInner {
            operations,
            old_token: friend.directional.new_token.clone(),
            rand_nonce,
        };

        let directional_mutation = DirectionalMutation::SetDirection(
            MoveTokenDirection::Outgoing(friend_move_token_inner));
        let friend_mutation = FriendMutation::DirectionalMutation(directional_mutation);
        let messenger_mutation = FunderMutation::FriendMutation((remote_public_key.clone(), friend_mutation));
        self.apply_mutation(messenger_mutation);

        let friend = self.get_friend(remote_public_key).unwrap();
        let outgoing_move_token = friend.directional.get_outgoing_move_token().unwrap();

        // Add a task for sending the outgoing move token:
        self.add_task(
            FunderTask::FriendMessage(
                FriendMessage::MoveToken(outgoing_move_token)));
    }


    /// Clear all pending inconsistency errors if exist
    fn clear_inconsistency_status(&mut self,
                               remote_public_key: &PublicKey) {
        let friend = self.get_friend(remote_public_key).unwrap();
        match friend.inconsistency_status.incoming {
            IncomingInconsistency::Empty => {},
            _ => {
                let friend_mutation = FriendMutation::SetIncomingInconsistency(IncomingInconsistency::Empty);
                let messenger_mutation = FunderMutation::FriendMutation((remote_public_key.clone(), friend_mutation));
                self.apply_mutation(messenger_mutation);
            },
        }

        let friend = self.get_friend(remote_public_key).unwrap();
        match friend.inconsistency_status.outgoing {
            OutgoingInconsistency::Empty => {},
            _ => {
                let friend_mutation = FriendMutation::SetOutgoingInconsistency(OutgoingInconsistency::Empty);
                let messenger_mutation = FunderMutation::FriendMutation((remote_public_key.clone(), friend_mutation));
                self.apply_mutation(messenger_mutation);
            },
        }
    }

    /// Handle success with incoming move token.
    #[async]
    fn handle_move_token_success(mut self,
                               remote_public_key: PublicKey,
                               receive_move_token_output: ReceiveMoveTokenOutput,
                               is_empty: bool) -> Result<Self, HandleFriendError> {

        self.clear_inconsistency_status(&remote_public_key);
                                        

        match receive_move_token_output {
            ReceiveMoveTokenOutput::Duplicate => Ok(self),
            ReceiveMoveTokenOutput::RetransmitOutgoing(outgoing_move_token) => {
                // Retransmit last sent token channel message:
                self.funder_tasks.push(
                    FunderTask::FriendMessage(
                        FriendMessage::MoveToken(outgoing_move_token)));
                Ok(self)
            },
            ReceiveMoveTokenOutput::Received(move_token_received) => {

                let MoveTokenReceived {incoming_messages, mutations} = 
                    move_token_received;

                // Apply all mutations:
                for directional_mutation in mutations {
                    let friend_mutation = FriendMutation::DirectionalMutation(directional_mutation);
                    let messenger_mutation = FunderMutation::FriendMutation((remote_public_key.clone(), friend_mutation));
                    self.apply_mutation(messenger_mutation);
                }

                let mut fself = await!(self.handle_move_token_output(remote_public_key.clone(),
                                               incoming_messages))?;
                fself.send_through_token_channel(&remote_public_key,
                                                 is_empty);
                Ok(fself)
            },
        }
    }


    #[async]
    fn handle_move_token(mut self, 
                         remote_public_key: PublicKey,
                         friend_move_token: FriendMoveToken) -> Result<Self,HandleFriendError> {

        // Find friend:
        let friend = match self.get_friend(&remote_public_key) {
            Some(friend) => friend,
            None => return Ok(self),
        };

        // Check if the channel is inconsistent.
        // This means that the remote side has sent an InconsistencyError message in the past.
        // In this case, we are not willing to accept new messages from the remote side until the
        // inconsistency is resolved.
        // TODO: Is this the correct behaviour?
        /*
        if let TokenChannelStatus::Inconsistent { .. } 
                    = token_channel_slot.tc_status {
            return Ok(self);
        };
        */


        let mut fself = await!(self.check_reset_channel(remote_public_key.clone(), 
                                           friend_move_token.new_token.clone()))?;

        let friend = fself.get_friend(&remote_public_key).unwrap();

        let is_empty = friend_move_token.operations.is_empty();

        // TODO: Possibly refactor this part into a function?
        let friend_move_token_inner = FriendMoveTokenInner {
            operations: friend_move_token.operations,
            old_token: friend_move_token.old_token,
            rand_nonce: friend_move_token.rand_nonce,
        };
        let receive_move_token_res = friend.directional.simulate_receive_move_token(
            friend_move_token_inner,
            friend_move_token.new_token);

        Ok(match receive_move_token_res {
            Ok(receive_move_token_output) => {
                await!(fself.handle_move_token_success(remote_public_key.clone(),
                                             receive_move_token_output,
                                             is_empty))?
            },
            Err(receive_move_token_error) => {
                fself.handle_move_token_error(&remote_public_key,
                                             receive_move_token_error);
                fself
            },
        })
    }

    fn handle_inconsistency_error(&mut self, 
                                  remote_public_key: &PublicKey,
                                  friend_inconsistency_error: FriendInconsistencyError) {
        
        // Save incoming inconsistency details:
        let incoming = IncomingInconsistency::Incoming(ResetTerms {
            current_token: friend_inconsistency_error.current_token.clone(),
            balance_for_reset: friend_inconsistency_error.balance_for_reset,
        });

        let friend_mutation = FriendMutation::SetIncomingInconsistency(incoming);
        let messenger_mutation = FunderMutation::FriendMutation((remote_public_key.clone(), friend_mutation));
        self.apply_mutation(messenger_mutation);

        // Obtain information about our reset terms:
        let friend = self.get_friend(remote_public_key).unwrap();
        let directional = &friend.directional;
        let reset_token = directional.calc_channel_reset_token();
        let balance_for_reset = directional.balance_for_reset();


        // Check if we should send an outgoing inconsistency message:
        let should_send_outgoing = match friend.inconsistency_status.outgoing {
            OutgoingInconsistency::Empty => {
                let friend_mutation = FriendMutation::SetOutgoingInconsistency(OutgoingInconsistency::Sent);
                let messenger_mutation = FunderMutation::FriendMutation((remote_public_key.clone(), friend_mutation));
                self.apply_mutation(messenger_mutation);
                true
            },
            OutgoingInconsistency::Sent => {
                let is_ack_valid = match friend_inconsistency_error.opt_ack {
                    Some(acked_reset_token) => acked_reset_token == reset_token,
                    None => false,
                };
                if is_ack_valid {
                    let friend_mutation = FriendMutation::SetOutgoingInconsistency(OutgoingInconsistency::Acked);
                    let messenger_mutation = FunderMutation::FriendMutation((remote_public_key.clone(), friend_mutation));
                    self.apply_mutation(messenger_mutation);
                    false
                } else {
                    true
                }
                
            },
            OutgoingInconsistency::Acked => false,
        };

        // Send an outgoing inconsistency message if required:
        if should_send_outgoing {
            let inconsistency_error = FriendInconsistencyError {
                opt_ack: Some(friend_inconsistency_error.current_token.clone()),
                current_token: reset_token.clone(),
                balance_for_reset,
            };

            self.add_task(
                FunderTask::FriendMessage(
                    FriendMessage::InconsistencyError(inconsistency_error)));
        }

    }

    #[async]
    pub fn handle_friend_message(mut self, 
                                   remote_public_key: PublicKey, 
                                   friend_message: IncomingFriendMessage)
                                        -> Result<Self, HandleFriendError> {
        match friend_message {
            IncomingFriendMessage::MoveToken(friend_move_token) =>
                await!(self.handle_move_token(remote_public_key, friend_move_token)),
            IncomingFriendMessage::InconsistencyError(friend_inconsistency_error) => {
                self.handle_inconsistency_error(&remote_public_key, friend_inconsistency_error);
                Ok(self)
            }
        }
    }

}
