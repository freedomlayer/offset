use std::rc::Rc;

use futures;
use futures::{Future, future};

use ring::rand::SecureRandom;

use crypto::rand_values::RandValue;
use crypto::identity::PublicKey;
use crypto::uid::Uid;

use proto::networker::ChannelToken;

use super::super::token_channel::incoming::ProcessOperationOutput;
use super::super::token_channel::directional::{ReceiveMoveTokenOutput, ReceiveMoveTokenError};
use super::{MessengerHandler, MessengerTask, NeighborMessage, AppManagerMessage,
            CrypterMessage, ResponseReceived};
use super::super::types::{NeighborTcOp, 
    RequestSendMessage, ResponseSendMessage, FailureSendMessage, RandNonceSignature, NeighborMoveToken};
use super::super::messenger_state::{NeighborState, StateMutateMessage, 
    MessengerStateError, TokenChannelStatus, TokenChannelSlot,
    SmInitTokenChannel, SmTokenChannelPushOp, SmResetTokenChannel, 
    SmApplyNeighborMoveToken};

use super::super::signature_buff::create_failure_signature_buffer;


#[allow(unused)]
pub struct NeighborInconsistencyError {
    token_channel_index: u16,
    current_token: ChannelToken,
    balance_for_reset: i64,
}

#[allow(unused)]
pub struct NeighborSetMaxTokenChannels {
    max_token_channels: u16,
}

#[allow(unused)]
pub enum IncomingNeighborMessage {
    MoveToken(NeighborMoveToken),
    InconsistencyError(NeighborInconsistencyError),
    SetMaxTokenChannels(NeighborSetMaxTokenChannels),
}

#[allow(unused)]
impl<R: SecureRandom + 'static> MessengerHandler<R> {

    fn get_token_channel_slot(&self, 
                              neighbor_public_key: &PublicKey,
                              channel_index: u16) -> &TokenChannelSlot {

        let neighbor = self.state.get_neighbors().get(&neighbor_public_key)
            .expect("Neighbor not found!");
        neighbor.token_channel_slots
            .get(&channel_index)
            .expect("token_channel_slot not found!")
    }

    /// Find the token channel in which a remote pending request resides
    /// Returns the index of the found token channel, or None if not found.
    fn find_token_channel_by_request_id(&self, 
                                        neighbor: &NeighborState, 
                                        request_id: &Uid) -> Option<u16> {

        for (&channel_index, token_channel_slot) in &neighbor.token_channel_slots {
            let pending_remote_requests = token_channel_slot.tc_state
                .get_token_channel()
                .pending_remote_requests();
            if pending_remote_requests.get(request_id).is_none() {
                return Some(channel_index)
            }
        }
        None
    }


    /// Find the originator of a pending local request.
    /// This should be a pending remote request at some other neighbor.
    /// Returns the public key of a neighbor together with the channel_index of a
    /// token channel. If we are the origin of this request, the function return None.
    ///
    /// TODO: We need to change this search to be O(1) in the future. Possibly by maintaining a map
    /// between request_id and (neighbor_public_key, neighbor).
    fn find_request_origin(&self, request_id: &Uid) -> Option<(PublicKey, u16)> {

        for (neighbor_public_key, neighbor) in self.state.get_neighbors() {
            match self.find_token_channel_by_request_id(&neighbor, request_id) {
                Some(channel_index) => return Some((neighbor_public_key.clone(), channel_index)),
                None => {},
            }
        }
        None
    }

    fn cancel_local_pending_requests(mut self, 
                                     neighbor_public_key: PublicKey, 
                                     channel_index: u16)
            -> Box<Future<Item=Self, Error=()>> {

        let neighbor = self.state.get_neighbors().get(&neighbor_public_key)
            .expect("Neighbor not found!");
        let token_channel_slot = neighbor.token_channel_slots
            .get(&channel_index)
            .expect("token_channel_slot not found!");

        // Mark all pending requests to this neighbor as errors.
        // As the token channel is being reset, we can be sure we will never obtain a response
        // for those requests.
        let pending_local_requests = token_channel_slot.tc_state
            .get_token_channel()
            .pending_local_requests();

        // Prepare a list of all remote requests that we need to cancel:
        let mut requests_to_cancel = Vec::new();
        for (local_request_id, pending_local_request) in pending_local_requests {
            let origin = self.find_request_origin(&local_request_id);
            let (origin_public_key, origin_channel_index) = match origin {
                Some((public_key, channel_index)) => (public_key, channel_index),
                None => continue,
            };
            requests_to_cancel.push((origin_public_key.clone(), 
                                     origin_channel_index, pending_local_request.clone()));
        }

        let local_public_key = self.state.get_local_public_key().clone();
        let rng = Rc::clone(&self.rng);
        let security_module_client = self.security_module_client.clone();

        // Create a future that will resolve to all the mutation messages.
        // We use futures here because the calculation of signature is futuristic.
        let fut_push_ops = 
            requests_to_cancel.into_iter()
            .map(move |(origin_public_key, origin_channel_index, pending_local_request)| {
                let failure_send_msg = FailureSendMessage {
                    request_id: pending_local_request.request_id,
                    reporting_public_key: local_public_key.clone(),
                    rand_nonce_signatures: Vec::new(), 
                };
                let mut failure_signature_buffer = create_failure_signature_buffer(
                                                    &failure_send_msg,
                                                    &pending_local_request);
                let rand_nonce = RandValue::new(&*rng);
                failure_signature_buffer.extend_from_slice(&rand_nonce);

                let local_public_key_cloned = local_public_key.clone();
                security_module_client
                    .request_signature(failure_signature_buffer)
                    .map_err(|e| -> () {panic!("Failed to create signature!")})
                    .and_then(move |signature| {
                        let rand_nonce_signature = RandNonceSignature {
                            rand_nonce,
                            signature,
                        };
                        let failure_send_msg = FailureSendMessage {
                            request_id: pending_local_request.request_id,
                            reporting_public_key: local_public_key_cloned,
                            rand_nonce_signatures: vec![rand_nonce_signature],
                        };
                        let failure_op = NeighborTcOp::FailureSendMessage(failure_send_msg);
                        Ok(SmTokenChannelPushOp {
                            neighbor_public_key: origin_public_key.clone(),
                            channel_index: origin_channel_index,
                            neighbor_op: failure_op,
                        })
                    })
                });

        Box::new(futures::collect(fut_push_ops)
            .and_then(|push_ops| {
                for token_channel_push_op in push_ops {
                    let sm_msg = StateMutateMessage::TokenChannelPushOp(token_channel_push_op.clone());
                    self.state.token_channel_push_op(token_channel_push_op)
                        .expect("Could not push neighbor operation into channel!");
                    self.sm_messages.push(sm_msg);
                }
                Ok(self)
            })
        )
    }


    /// Check if channel reset is required (Remove side used the RESET token)
    /// If so, reset the channel.
    fn check_reset_channel(mut self, 
                           neighbor_public_key: PublicKey,
                           channel_index: u16,
                           new_token: ChannelToken)
            -> Box<Future<Item=Self, Error=()>> {
        // Check if incoming message is an attempt to reset channel.
        // We can know this by checking if new_token is a special value.
        let token_channel_slot = self.get_token_channel_slot(&neighbor_public_key,
                                                             channel_index);
        let reset_token = token_channel_slot.tc_state.calc_channel_reset_token(channel_index);
        let balance_for_reset = token_channel_slot.tc_state.balance_for_reset();

        if new_token == reset_token {
            // This is a reset message. We reset the token channel:
            let fut = self.cancel_local_pending_requests(
                neighbor_public_key.clone(), channel_index)
            .and_then(move |mut fself| {
                let reset_token_channel = SmResetTokenChannel {
                    neighbor_public_key: neighbor_public_key.clone(),
                    channel_index, 
                    reset_token,
                    balance_for_reset,
                };
                let sm_msg = StateMutateMessage::ResetTokenChannel(reset_token_channel.clone());
                fself.state.reset_token_channel(reset_token_channel);
                fself.sm_messages.push(sm_msg);
                Ok(fself)
            });
            Box::new(fut) as Box<Future<Item=Self, Error=()>>
        } else {
            Box::new(future::ok(self)) as Box<Future<Item=Self, Error=()>>
        }
    }

    fn handle_request_send_msg(&mut self, 
                               remote_public_key: &PublicKey,
                               channel_index: u16,
                               request_send_msg: RequestSendMessage) {
        // TODO
        //  - If we are the last on the route, punt to the Crypter.
        //  - Perform DoS protection check.
        //      - If valid, Queue to correct token channel.
        //      - If invalid, Queue a failure message to this token channel.
        unreachable!();
    }

    fn handle_response_send_msg(&mut self, 
                               remote_public_key: &PublicKey,
                               channel_index: u16,
                               response_send_msg: ResponseSendMessage) {

        match self.find_request_origin(&response_send_msg.request_id) {
            None => {
                // We are the origin of this request, and we got a response.
                // We should pass it back to crypter.
                self.messenger_tasks.push(
                    MessengerTask::CrypterMessage(
                        CrypterMessage::ResponseReceived(ResponseReceived {
                            request_id: response_send_msg.request_id,
                            processing_fee_collected: response_send_msg.processing_fee_collected,
                            response_content: response_send_msg.response_content,
                        })
                    )
                );
            },
            Some((neighbor_public_key, channel_index)) => {
                // Queue this response message to another token channel:
                let response_op = NeighborTcOp::ResponseSendMessage(response_send_msg);
                let push_op = SmTokenChannelPushOp {
                    neighbor_public_key,
                    channel_index,
                    neighbor_op: response_op,
                };

                let sm_msg = StateMutateMessage::TokenChannelPushOp(push_op.clone());
                self.state.token_channel_push_op(push_op)
                    .expect("Could not push neighbor operation into channel!");
                self.sm_messages.push(sm_msg);
            },
        }
    }

    fn handle_failure_send_msg(&mut self, 
                               remote_public_key: &PublicKey,
                               channel_index: u16,
                               failure_send_msg: FailureSendMessage) {
        // TODO
        // - Queue to correct token channel.
        unreachable!();
    }

    /// Process valid incoming operations from remote side.
    fn handle_move_token_output(mut self, 
                                remote_public_key: PublicKey,
                                channel_index: u16,
                                ops_list_output: Vec<ProcessOperationOutput> )
                        -> Box<Future<Item=Self, Error=()>> {

        for op_output in ops_list_output {
            match op_output {
                ProcessOperationOutput::Request(request_send_msg) => 
                    self.handle_request_send_msg(&remote_public_key, channel_index, request_send_msg),
                ProcessOperationOutput::Response(response_send_msg) => 
                    self.handle_response_send_msg(&remote_public_key, channel_index, response_send_msg),
                ProcessOperationOutput::Failure(failure_send_msg) =>
                    self.handle_failure_send_msg(&remote_public_key, channel_index, failure_send_msg),
            }
        }
        Box::new(future::ok(self))
    }

    /// Handle an error with incoming move token.
    fn handle_move_token_error(&mut self,
                               remote_public_key: &PublicKey,
                               channel_index: u16,
                               receive_move_token_error: ReceiveMoveTokenError) {
        // Send a message about inconsistency problem to AppManager:
        self.messenger_tasks.push(
            MessengerTask::AppManagerMessage(
                AppManagerMessage::ReceiveMoveTokenError(receive_move_token_error)));

        let token_channel_slot = self.get_token_channel_slot(&remote_public_key, 
                                                              channel_index);
        // Send an InconsistencyError message to remote side:
        let current_token = token_channel_slot.tc_state
            .calc_channel_reset_token(channel_index);
        let balance_for_reset = token_channel_slot.tc_state
            .balance_for_reset();

        let inconsistency_error = NeighborInconsistencyError {
            token_channel_index: channel_index,
            current_token,
            balance_for_reset,
        };

        self.messenger_tasks.push(
            MessengerTask::NeighborMessage(
                NeighborMessage::InconsistencyError(inconsistency_error)));
    }


    /// Compose a large as possible message to send through the token channel to the remote side.
    /// The message should contain various operations, collected from:
    /// - Generic pending requests (Might be sent through any token channel).
    /// - Token channel specific pending responses/failures.
    /// - Commands that were initialized through AppManager.
    ///
    /// Any operations that will enter the message should be applied. For example, a failure
    /// message should cause the pending request to be removed.
    fn send_through_token_channel(&mut self, 
                                  remote_public_key: &PublicKey,
                                  channel_index: u16) {

        // - If any messages are pending for this token channel, batch as many as possible into one
        //   move token message and add a task to send it. 
        //   - The first messages in the batch should be pending configuration requests:
        //      - Set remote max debt
        //      - Open, Close neighbor for requests.
        // TODO
        unreachable!();
    }

    /// Initialte loading funds (Using a funder message) for token channel, if needed.
    fn initiate_load_funds(&mut self,
                           remote_public_key: &PublicKey,
                           channel_index: u16) {
        // TODO
        unreachable!();
    }


    fn handle_move_token(mut self, 
                         remote_public_key: PublicKey,
                         neighbor_move_token: NeighborMoveToken) 
         -> Box<Future<Item=Self, Error=()>> {

        // Find neighbor:
        let neighbor = match self.state.get_neighbors().get(&remote_public_key) {
            Some(neighbor) => neighbor,
            None => return Box::new(future::ok(self)),
        };

        let channel_index = neighbor_move_token.token_channel_index;
        if channel_index >= neighbor.local_max_channels {
            // Tell remote side that we don't support such a high token channel index:
            self.messenger_tasks.push(
                MessengerTask::NeighborMessage(
                    NeighborMessage::SetMaxTokenChannels(
                        NeighborSetMaxTokenChannels {
                            max_token_channels: neighbor.local_max_channels,
                        }
                    )
                )
            );
            return Box::new(future::ok(self));
        }


        if !neighbor.token_channel_slots.contains_key(&channel_index) {
            let init_token_channel = SmInitTokenChannel {
                neighbor_public_key: remote_public_key.clone(),
                channel_index,
            };
            let sm_msg = StateMutateMessage::InitTokenChannel(init_token_channel.clone());
            self.state.init_token_channel(init_token_channel)
                .expect("Failed to initialize token channel!");
            self.sm_messages.push(sm_msg);
        }

        let token_channel_slot = self.get_token_channel_slot(&remote_public_key, 
                                                             channel_index);

        // QUESTION: Should Database be informed about the creation of a new token channel?
        // This is not really a creation of anything new, as we create the default new channel.

        // Check if the channel is inconsistent.
        // This means that the remote side has sent an InconsistencyError message in the past.
        // In this case, we are not willing to accept new messages from the remote side until the
        // inconsistency is resolved.
        if let TokenChannelStatus::Inconsistent { .. } 
                    = token_channel_slot.tc_status {
            return Box::new(future::ok(self));
        };

        let fut = self.check_reset_channel(remote_public_key.clone(), 
                                           channel_index, 
                                           neighbor_move_token.new_token.clone());

        Box::new(fut.and_then(move |mut fself| {
            let apply_neighbor_move_token = SmApplyNeighborMoveToken {
                neighbor_public_key: remote_public_key.clone(),
                neighbor_move_token,
            };
            let sm_msg = StateMutateMessage::ApplyNeighborMoveToken(apply_neighbor_move_token.clone());
            match fself.state.apply_neighbor_move_token(apply_neighbor_move_token) {
                Ok(ReceiveMoveTokenOutput::Duplicate) => Box::new(future::ok(fself)),
                Ok(ReceiveMoveTokenOutput::RetransmitOutgoing(outgoing_move_token)) => {
                    // Retransmit last sent token channel message:
                    fself.messenger_tasks.push(
                        MessengerTask::NeighborMessage(
                            NeighborMessage::MoveToken(outgoing_move_token)));
                    Box::new(future::ok(fself))
                },
                Ok(ReceiveMoveTokenOutput::ProcessOpsListOutput(ops_list_output)) => {
                    Box::new(fself.handle_move_token_output(remote_public_key.clone(),
                                                   channel_index,
                                                   ops_list_output)
                    .and_then(move |mut fself| {
                        fself.send_through_token_channel(&remote_public_key,
                                                         channel_index);
                        fself.initiate_load_funds(&remote_public_key,
                                                  channel_index);
                        Ok(fself)
                    })) as Box<Future<Item=Self,Error=()>>
                },
                Err(MessengerStateError::ReceiveMoveTokenError(receive_move_token_error)) => {
                    fself.handle_move_token_error(&remote_public_key,
                                                 channel_index,
                                                 receive_move_token_error);
                    Box::new(future::ok(fself))
                },
                Err(_) => unreachable!(),
            }
        })) as Box<Future<Item=Self,Error=()>>
    }

    fn handle_inconsistency_error(self, 
                                  remote_public_key: PublicKey,
                                  neighbor_inconsistency_error: NeighborInconsistencyError)
         -> Box<Future<Item=Self, Error=()>> {

        unreachable!();
        Box::new(future::ok(self))
    }

    fn handle_set_max_token_channels(self, 
                                     remote_public_key: PublicKey,
                                     neighbor_set_max_token_channels: NeighborSetMaxTokenChannels)
         -> Box<Future<Item=Self, Error=()>> {
        unreachable!();
        Box::new(future::ok(self))
    }

    pub fn handle_neighbor_message(self, 
                                   remote_public_key: PublicKey, 
                                   neighbor_message: IncomingNeighborMessage)
         -> Box<Future<Item=Self, Error=()>> {

        match neighbor_message {
            IncomingNeighborMessage::MoveToken(neighbor_move_token) =>
                self.handle_move_token(remote_public_key, neighbor_move_token),
            IncomingNeighborMessage::InconsistencyError(neighbor_inconsistency_error) =>
                self.handle_inconsistency_error(remote_public_key, neighbor_inconsistency_error),
            IncomingNeighborMessage::SetMaxTokenChannels(neighbor_set_max_token_channels) =>
                self.handle_set_max_token_channels(remote_public_key, neighbor_set_max_token_channels),
        }
    }
}
