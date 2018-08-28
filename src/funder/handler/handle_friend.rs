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
    DirectionalMutation, MoveTokenDirection, MoveTokenReceived, SetDirection};
use super::{MutableFunderHandler, FunderTask, FriendMessage,
            ResponseReceived};
use super::super::types::{FriendTcOp, RequestSendFunds, 
    ResponseSendFunds, FailureSendFunds, 
    FriendMoveToken, FunderFreezeLink, PkPairPosition, 
    PendingFriendRequest, Ratio, RequestsStatus};

use super::super::state::FunderMutation;
use super::super::friend::{FriendState, FriendMutation, 
    OutgoingInconsistency, IncomingInconsistency, 
    ResetTerms, ResponseOp};

use super::super::signature_buff::{create_failure_signature_buffer, prepare_receipt};
use super::super::messages::ResponseSendFundsResult;

use super::super::liveness::Actions;
use super::FriendInconsistencyError;

use proto::common::SendFundsReceipt;


#[derive(Debug)]
pub enum HandleFriendError {
    FriendDoesNotExist,
    NoMoveTokenToAck,
    AlreadyAcked,
    TokenNotOwned,
    IncorrectAckedToken,
    IncorrectLastToken,
}


#[allow(unused)]
impl<A: Clone + 'static, R: SecureRandom + 'static> MutableFunderHandler<A,R> {

    /// Check if channel reset is required (Remove side used the RESET token)
    /// If so, reset the channel.
    #[async]
    fn check_reset_channel(mut self, 
                           friend_public_key: PublicKey,
                           friend_move_token: FriendMoveToken) -> Result<Self, HandleFriendError> {
        // Check if incoming message is an attempt to reset channel.
        // We can know this by checking if old_token is a special value.
        let friend = self.get_friend(&friend_public_key).unwrap();
        let reset_token = friend.directional.calc_channel_reset_token();
        let balance_for_reset = friend.directional.balance_for_reset();

        if friend_move_token.old_token == reset_token {
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
        let friend_mutation = FriendMutation::PushBackPendingRequest(request_send_funds.clone());
        let messenger_mutation = FunderMutation::FriendMutation((next_pk.clone(), friend_mutation));
        self.apply_mutation(messenger_mutation);
        self.try_send_channel(&next_pk);
    }

    #[async]
    fn handle_request_send_funds(mut self, 
                               remote_public_key: PublicKey,
                               request_send_funds: RequestSendFunds) -> Result<Self, !> {

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
        let friend_exists = !self.state.get_friends().contains_key(next_public_key);

        // This friend must be considered online.
        let friend_online = if friend_exists {
            self.ephemeral.liveness.friends
                .get(&next_public_key)
                .unwrap()
                .is_online()
        } else {
            false
        };

        let mut fself = if !friend_online {
            await!(self.reply_with_failure(remote_public_key.clone(), 
                                           request_send_funds.clone()))?
        } else {
            self
        };

        // Perform DoS protection check:
        Ok(match fself.ephemeral.freeze_guard.verify_freezing_links(&request_send_funds) {
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

        match self.find_request_origin(&response_send_funds.request_id).cloned() {
            None => {
                // We are the origin of this request, and we got a response.
                // We should pass it back to crypter.


                let receipt = prepare_receipt(&response_send_funds,
                                              &pending_request);

                let response_send_funds_result = ResponseSendFundsResult::Success(receipt);
                self.add_task(
                    FunderTask::ResponseReceived(
                        ResponseReceived {
                            request_id: pending_request.request_id,
                            result: response_send_funds_result,
                        }
                    )
                );
            },
            Some(friend_public_key) => {
                // Queue this response message to another token channel:
                let response_op = ResponseOp::Response(response_send_funds);
                let friend_mutation = FriendMutation::PushBackPendingResponse(response_op);
                let messenger_mutation = FunderMutation::FriendMutation((friend_public_key.clone(), friend_mutation));
                self.apply_mutation(messenger_mutation);
                self.try_send_channel(&friend_public_key);
            },
        }
    }

    #[async]
    fn handle_failure_send_funds(mut self, 
                               remote_public_key: &PublicKey,
                               failure_send_funds: FailureSendFunds,
                               pending_request: PendingFriendRequest)
                                -> Result<Self, !> {

        let fself = match self.find_request_origin(&failure_send_funds.request_id).cloned() {
            None => {
                // We are the origin of this request, and we got a failure
                // We should pass it back to crypter.


                let response_send_funds_result = ResponseSendFundsResult::Failure(failure_send_funds.reporting_public_key);
                self.funder_tasks.push(
                    FunderTask::ResponseReceived(
                        ResponseReceived {
                            request_id: pending_request.request_id,
                            result: response_send_funds_result,
                        }
                    )
                );

                self
            },
            Some(friend_public_key) => {
                // Queue this failure message to another token channel:
                let failure_op = ResponseOp::Failure(failure_send_funds);
                let friend_mutation = FriendMutation::PushBackPendingResponse(failure_op);
                let messenger_mutation = FunderMutation::FriendMutation((friend_public_key.clone(), friend_mutation));
                self.apply_mutation(messenger_mutation);
                self.try_send_channel(&friend_public_key);

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
                        -> Result<Self, !> {

        let mut fself = self;
        for incoming_message in incoming_messages {
            fself = match incoming_message {
                IncomingMessage::Request(request_send_funds) => {
                    await!(fself.handle_request_send_funds(remote_public_key.clone(),
                                                 request_send_funds))?
                },
                IncomingMessage::Response(IncomingResponseSendFunds {
                                                pending_request, incoming_response}) => {
                    fself.ephemeral.freeze_guard.sub_frozen_credit(&pending_request);
                    fself.handle_response_send_funds(&remote_public_key, 
                                                  incoming_response, pending_request);
                    fself
                },
                IncomingMessage::Failure(IncomingFailureSendFunds {
                                                pending_request, incoming_failure}) => {
                    fself.ephemeral.freeze_guard.sub_frozen_credit(&pending_request);
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

        let reset_terms = ResetTerms {
            current_token: current_token.clone(),
            balance_for_reset,
        };

        let inconsistency_error = FriendInconsistencyError {
            opt_ack: None,
            current_token,
            balance_for_reset,
        };

        self.funder_tasks.push(
            FunderTask::FriendMessage(
                FriendMessage::InconsistencyError(inconsistency_error)));
        let liveness_friend = self.ephemeral.liveness.friends.get_mut(&remote_public_key).unwrap();
        liveness_friend.reset_inconsistency();
        liveness_friend.cancel_token_msg();

        // Keep outgoing InconsistencyError message details in memory:
        let friend_mutation = FriendMutation::SetOutgoingInconsistency(OutgoingInconsistency::Sent(reset_terms));
        let messenger_mutation = FunderMutation::FriendMutation((remote_public_key.clone(), friend_mutation));
        self.apply_mutation(messenger_mutation);
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
                               receive_move_token_output: ReceiveMoveTokenOutput) 
        -> Result<Self, !> {

        self.clear_inconsistency_status(&remote_public_key);

        match receive_move_token_output {
            ReceiveMoveTokenOutput::Duplicate => {
                // Send an ack:
                let acked_token = self.get_friend(&remote_public_key)
                    .unwrap()
                    .directional
                    .new_token();

                self.add_task(
                    FunderTask::FriendMessage(
                        FriendMessage::MoveTokenAck(acked_token)));
                Ok(self)
            },
            ReceiveMoveTokenOutput::RetransmitOutgoing(outgoing_move_token) => {
                // Retransmit last sent token channel message:
                self.transmit_outgoing(&remote_public_key);
                Ok(self)
            },
            ReceiveMoveTokenOutput::Received(move_token_received) => {
                let liveness_friend = self.ephemeral.liveness.friends.get_mut(&remote_public_key).unwrap();
                liveness_friend.cancel_inconsistency();
                liveness_friend.cancel_request_token();

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
                if !fself.send_through_token_channel(&remote_public_key) {
                    // If we didn't reply a move token message, we will reply an ack:
                    let acked_token = fself.get_friend(&remote_public_key)
                        .unwrap()
                        .directional
                        .new_token();
                    fself.add_task(
                        FunderTask::FriendMessage(
                            FriendMessage::MoveTokenAck(acked_token)));
                }
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
            Some(friend) => Ok(friend),
            None => Err(HandleFriendError::FriendDoesNotExist),
        }?;

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
                                           friend_move_token.clone()))?;

        let friend = fself.get_friend(&remote_public_key).unwrap();

        let receive_move_token_res = friend.directional.simulate_receive_move_token(
            friend_move_token);

        Ok(match receive_move_token_res {
            Ok(receive_move_token_output) => {
                await!(fself.handle_move_token_success(remote_public_key.clone(),
                                             receive_move_token_output))?
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
                                  friend_inconsistency_error: FriendInconsistencyError)
                                    -> Result<(), HandleFriendError> {

        // Make sure that friend exists:
        let _ = match self.get_friend(&remote_public_key) {
            Some(friend) => Ok(friend),
            None => Err(HandleFriendError::FriendDoesNotExist),
        }?;
        // Save incoming inconsistency details:
        let incoming = IncomingInconsistency::Incoming(ResetTerms {
            current_token: friend_inconsistency_error.current_token.clone(),
            balance_for_reset: friend_inconsistency_error.balance_for_reset,
        });

        let friend_mutation = FriendMutation::SetIncomingInconsistency(incoming);
        let messenger_mutation = FunderMutation::FriendMutation((remote_public_key.clone(), friend_mutation));
        self.apply_mutation(messenger_mutation);

        // We stop resending token messages, because an inconsistency was received:
        let liveness_friend = self.ephemeral.liveness.friends.get_mut(&remote_public_key).unwrap();
        liveness_friend.cancel_token_msg();
        

        // Obtain information about our reset terms:
        let friend = self.get_friend(remote_public_key).unwrap();
        let directional = &friend.directional;
        let reset_token = directional.calc_channel_reset_token();
        let balance_for_reset = directional.balance_for_reset();

        let reset_terms = ResetTerms {
            current_token: reset_token.clone(),
            balance_for_reset,
        };

        // Check if we should send an outgoing inconsistency message:
        let should_send_outgoing = match friend.inconsistency_status.outgoing {
            OutgoingInconsistency::Empty => {
                let friend_mutation = FriendMutation::SetOutgoingInconsistency(OutgoingInconsistency::Sent(reset_terms));
                let messenger_mutation = FunderMutation::FriendMutation((remote_public_key.clone(), friend_mutation));
                self.apply_mutation(messenger_mutation);
                true
            },
            OutgoingInconsistency::Sent(_) => {
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
            let liveness_friend = self.ephemeral.liveness.friends.get_mut(&remote_public_key).unwrap();
            liveness_friend.reset_inconsistency();
        }
        Ok(())
    }

    fn handle_move_token_ack(&mut self, 
                                remote_public_key: &PublicKey,
                                acked_token: ChannelToken)
                                    -> Result<(), HandleFriendError> {
        // Find friend:
        let friend = match self.get_friend(&remote_public_key) {
            Some(friend) => Ok(friend),
            None => Err(HandleFriendError::FriendDoesNotExist),
        }?;

        let friend = self.get_friend(&remote_public_key).unwrap();

        // If we have the token, we ignore the ack:
        let outgoing_move_token = match &friend.directional.direction {
            MoveTokenDirection::Outgoing(outgoing_move_token) => Ok(outgoing_move_token),
            MoveTokenDirection::Incoming(_) => Err(HandleFriendError::NoMoveTokenToAck),
        }?;

        if outgoing_move_token.friend_move_token.old_token != acked_token {
            return Err(HandleFriendError::IncorrectAckedToken);
        }

        let directional_mutation = DirectionalMutation::AckOutgoing;
        let friend_mutation = FriendMutation::DirectionalMutation(directional_mutation);
        let messenger_mutation = FunderMutation::FriendMutation((remote_public_key.clone(), friend_mutation));
        self.apply_mutation(messenger_mutation);

        // Cancel retransmission of move token message, 
        // because we have received a valid ack:
        let liveness_friend = self.ephemeral.liveness.friends.get_mut(&remote_public_key).unwrap();
        liveness_friend.cancel_token_msg();

        Ok(())
    }

    fn handle_request_token(&mut self, 
                            remote_public_key: &PublicKey,
                            last_token: ChannelToken)
                                    -> Result<(), HandleFriendError> {
        // Find friend:
        let friend = match self.get_friend(&remote_public_key) {
            Some(friend) => Ok(friend),
            None => Err(HandleFriendError::FriendDoesNotExist),
        }?;

        // If remote side has the token, we ignore the request:
        let new_token = match &friend.directional.direction {
            MoveTokenDirection::Outgoing(_) => Err(HandleFriendError::TokenNotOwned),
            MoveTokenDirection::Incoming(new_token) => Ok(new_token),
        }?;

        if *new_token != last_token {
            return Err(HandleFriendError::IncorrectLastToken);
        }

        // Compose an empty friend_move_token message and send it to the remote side:
        let move_token_sent = self.send_friend_move_token(
            &remote_public_key, Vec::new()).unwrap();

        Ok(())
    }

    fn handle_keep_alive(&mut self, 
                        remote_public_key: &PublicKey)
                                    -> Result<(), HandleFriendError> {
        Ok(())
    }

    #[async]
    pub fn handle_friend_message(mut self, 
                                   remote_public_key: PublicKey, 
                                   friend_message: FriendMessage)
                                        -> Result<Self, HandleFriendError> {

        // Make sure that friend exists:
        let _ = match self.get_friend(&remote_public_key) {
            Some(friend) => Ok(friend),
            None => Err(HandleFriendError::FriendDoesNotExist),
        }?;

        let liveness_friend = self.ephemeral.liveness.friends
            .get_mut(&remote_public_key)
            .unwrap();
        liveness_friend.message_received();

        let mut fself = match friend_message {
            FriendMessage::MoveToken(friend_move_token) =>
                await!(self.handle_move_token(remote_public_key.clone(), friend_move_token)),
            FriendMessage::InconsistencyError(friend_inconsistency_error) => {
                self.handle_inconsistency_error(&remote_public_key.clone(), friend_inconsistency_error);
                Ok(self)
            }
            FriendMessage::MoveTokenAck(acked_token) => {
                self.handle_move_token_ack(&remote_public_key, acked_token)?;
                Ok(self)
            },
            FriendMessage::RequestToken(last_token) => {
                self.handle_request_token(&remote_public_key, last_token)?;
                Ok(self)
            },
            FriendMessage::KeepAlive => {
                self.handle_keep_alive(&remote_public_key)?;
                Ok(self)
            },
        }?;

        // If any outgoing message was queued as a task, we mark that a message was sent:
        if fself.has_outgoing_message() {
            let liveness_friend = fself.ephemeral.liveness.friends.get_mut(&remote_public_key).unwrap();
            liveness_friend.message_sent();
        }
        Ok(fself)
    }
}
