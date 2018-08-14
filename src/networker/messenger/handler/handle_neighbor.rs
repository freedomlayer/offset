use std::convert::TryFrom;

use futures::prelude::{async, await};

use num_bigint::BigUint;
use num_traits::ToPrimitive;

use ring::rand::SecureRandom;

use crypto::rand_values::RandValue;
use crypto::identity::PublicKey;
use crypto::uid::Uid;

use utils::safe_arithmetic::SafeArithmetic;


use proto::networker::ChannelToken;

use super::super::token_channel::incoming::{IncomingResponseSendMessage, 
    IncomingFailureSendMessage, IncomingMessage};
use super::super::token_channel::outgoing::{OutgoingTokenChannel, QueueOperationFailure,
    QueueOperationError};
use super::super::token_channel::directional::{ReceiveMoveTokenOutput, ReceiveMoveTokenError, 
    DirectionalMutation, MoveTokenDirection, MoveTokenReceived};
use super::{MutableMessengerHandler, MessengerTask, NeighborMessage, AppManagerMessage,
            CrypterMessage, RequestReceived, ResponseReceived, FailureReceived, SendPayment};
use super::super::types::{NeighborTcOp, RequestSendMessage, 
    ResponseSendMessage, FailureSendMessage, RandNonceSignature, 
    NeighborMoveToken};
use super::super::token_channel::types::NeighborMoveTokenInner;
use super::super::state::MessengerMutation;
use super::super::neighbor::{NeighborState, NeighborMutation};
use super::super::slot::{TokenChannelSlot, SlotMutation, OutgoingInconsistency, 
    IncomingInconsistency, ResetTerms};

use super::super::signature_buff::create_failure_signature_buffer;
use super::super::types::{NetworkerFreezeLink, PkPairPosition, PendingNeighborRequest, Ratio};


// Approximate maximum size of a MOVE_TOKEN message.
// TODO: Where to put this constant? Do we have more like this one?
const MAX_MOVE_TOKEN_LENGTH: usize = 0x1000;


#[allow(unused)]
pub struct NeighborInconsistencyError {
    opt_ack: Option<ChannelToken>,
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

pub enum HandleNeighborError {
}




#[allow(unused)]
impl<R: SecureRandom + 'static> MutableMessengerHandler<R> {

    fn get_token_channel_slot(&self, 
                              neighbor_public_key: &PublicKey,
                              channel_index: u16) -> &TokenChannelSlot {

        let neighbor = self.state.get_neighbors().get(&neighbor_public_key)
            .expect("Neighbor not found!");
        neighbor.tc_slots
            .get(&channel_index)
            .expect("token_channel_slot not found!")
    }

    /// Find the token channel in which a remote pending request resides
    /// Returns the index of the found token channel, or None if not found.
    fn find_token_channel_by_request_id(&self, 
                                        neighbor: &NeighborState, 
                                        request_id: &Uid) -> Option<u16> {

        for (channel_index, token_channel_slot) in &neighbor.tc_slots {
            let pending_remote_requests = &token_channel_slot.directional
                .token_channel.state().pending_requests.pending_remote_requests;
            if pending_remote_requests.get(request_id).is_none() {
                return Some(*channel_index)
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
            if let Some(channel_index) = self.find_token_channel_by_request_id(
                &neighbor, request_id) {
                return Some((neighbor_public_key.clone(), channel_index));
            }
        }
        None
    }

    /// Create a (signed) failure message for a given request_id.
    /// We are the reporting_public_key for this failure message.
    #[async]
    fn create_failure_message(mut self, pending_local_request: PendingNeighborRequest) 
        -> Result<(Self, FailureSendMessage), HandleNeighborError> {

        let local_public_key = self.state.get_local_public_key().clone();
        let failure_send_msg = FailureSendMessage {
            request_id: pending_local_request.request_id,
            reporting_public_key: local_public_key.clone(),
            rand_nonce_signatures: Vec::new(), 
        };
        let mut failure_signature_buffer = create_failure_signature_buffer(
                                            &failure_send_msg,
                                            &pending_local_request);
        let rand_nonce = RandValue::new(&*self.rng);
        failure_signature_buffer.extend_from_slice(&rand_nonce);

        let signature = await!(self.security_module_client.request_signature(failure_signature_buffer))
            .expect("Failed to create a signature!");

        let rand_nonce_signature = RandNonceSignature {
            rand_nonce,
            signature,
        };

        Ok((self, FailureSendMessage {
            request_id: pending_local_request.request_id,
            reporting_public_key: local_public_key,
            rand_nonce_signatures: vec![rand_nonce_signature],
        }))
    }

    #[async]
    fn failure_message_add_signature(mut self, 
                                     mut failure_send_msg: FailureSendMessage,
                                     pending_local_request: PendingNeighborRequest) 
        -> Result<(Self, FailureSendMessage),HandleNeighborError> {

        let mut failure_signature_buffer = create_failure_signature_buffer(
                                            &failure_send_msg,
                                            &pending_local_request);
        let rand_nonce = RandValue::new(&*self.rng);
        failure_signature_buffer.extend_from_slice(&rand_nonce);
        let signature = await!(self.security_module_client.request_signature(failure_signature_buffer))
            .expect("Failed to create a signature!");

        let rand_nonce_signature = RandNonceSignature {
            rand_nonce,
            signature,
        };
        failure_send_msg.rand_nonce_signatures.push(rand_nonce_signature);

        Ok((self, failure_send_msg))
    }


    #[async]
    fn cancel_local_pending_requests(mut self, 
                                     neighbor_public_key: PublicKey, 
                                     channel_index: u16) -> Result<Self, HandleNeighborError> {

        let neighbor = self.state.get_neighbors().get(&neighbor_public_key)
            .expect("Neighbor not found!");
        let token_channel_slot = neighbor.tc_slots
            .get(&channel_index)
            .expect("token_channel_slot not found!");

        // Mark all pending requests to this neighbor as errors.  
        // As the token channel is being reset, we can be sure we will never obtain a response
        // for those requests.
        let pending_local_requests = token_channel_slot.directional
            .token_channel
            .state()
            .pending_requests
            .pending_local_requests
            .clone();

        let local_public_key = self.state.get_local_public_key().clone();
        let mut fself = self;
        // Prepare a list of all remote requests that we need to cancel:
        for (local_request_id, pending_local_request) in pending_local_requests {
            let origin = fself.find_request_origin(&local_request_id);
            let (origin_public_key, origin_channel_index) = match origin {
                Some((public_key, channel_index)) => (public_key, channel_index),
                None => continue,
            };

            let (new_fself, failure_send_msg) = await!(fself.create_failure_message(pending_local_request))?;
            fself = new_fself;

            let failure_op = NeighborTcOp::FailureSendMessage(failure_send_msg);
            let slot_mutation = SlotMutation::PushBackPendingOperation(failure_op);
            let neighbor_mutation = NeighborMutation::SlotMutation((origin_channel_index, slot_mutation));
            let messenger_mutation = MessengerMutation::NeighborMutation((origin_public_key.clone(), neighbor_mutation));
            fself.apply_mutation(messenger_mutation);
        }
        Ok(fself)
   }


    /// Check if channel reset is required (Remove side used the RESET token)
    /// If so, reset the channel.
    #[async]
    fn check_reset_channel(mut self, 
                           neighbor_public_key: PublicKey,
                           channel_index: u16,
                           new_token: ChannelToken) -> Result<Self, HandleNeighborError> {
        // Check if incoming message is an attempt to reset channel.
        // We can know this by checking if new_token is a special value.
        let token_channel_slot = self.get_token_channel_slot(&neighbor_public_key,
                                                             channel_index);
        let reset_token = token_channel_slot.directional.calc_channel_reset_token(channel_index);
        let balance_for_reset = token_channel_slot.directional.balance_for_reset();

        if new_token == reset_token {
            // This is a reset message. We reset the token channel:
            let mut fself = await!(self.cancel_local_pending_requests(
                neighbor_public_key.clone(), channel_index))?;

            let slot_mutation = SlotMutation::RemoteReset;
            let neighbor_mutation = NeighborMutation::SlotMutation((channel_index, slot_mutation));
            let messenger_mutation = MessengerMutation::NeighborMutation((neighbor_public_key.clone(), neighbor_mutation));
            fself.apply_mutation(messenger_mutation);

            Ok(fself)
        } else {
            Ok(self)
        }
    }

    fn punt_request_to_crypter(&mut self, request_send_msg: RequestSendMessage) {
        self.messenger_tasks.push(
            MessengerTask::CrypterMessage(
                CrypterMessage::RequestReceived(RequestReceived {
                    request_id: request_send_msg.request_id,
                    route: request_send_msg.route,
                    request_content: request_send_msg.request_content,
                    max_response_len: request_send_msg.max_response_len,
                    processing_fee_proposal: request_send_msg.processing_fee_proposal,
                })
            )
        );
    }

    /// Reply to a request message with failure.
    #[async]
    fn reply_with_failure(self, 
                          remote_public_key: PublicKey,
                          channel_index: u16,
                          request_send_msg: RequestSendMessage) -> Result<Self, HandleNeighborError> {

        let pending_request = request_send_msg.create_pending_request()
            .expect("Could not create pending_request");
        let (mut fself, failure_send_msg) = await!(self.create_failure_message(pending_request))?;

        let failure_op = NeighborTcOp::FailureSendMessage(failure_send_msg);
        let slot_mutation = SlotMutation::PushBackPendingOperation(failure_op);
        let neighbor_mutation = NeighborMutation::SlotMutation((channel_index, slot_mutation));
        let messenger_mutation = MessengerMutation::NeighborMutation((remote_public_key.clone(), neighbor_mutation));
        fself.apply_mutation(messenger_mutation);

        Ok(fself)
    }

    /// Forward a request message to the relevant neighbor and token channel.
    fn forward_request(&mut self, mut request_send_msg: RequestSendMessage) {
        let index = request_send_msg.route.pk_index(self.state.get_local_public_key())
            .expect("We are not present in the route!");
        let prev_index = index.checked_sub(1).expect("We are the originator of this request");
        let next_index = index.checked_add(1).expect("Index out of range");
        
        let prev_pk = request_send_msg.route.pk_by_index(prev_index)
            .expect("Could not obtain previous public key");
        let next_pk = request_send_msg.route.pk_by_index(prev_index)
            .expect("Could not obtain next public key");

        let prev_neighbor = self.state.get_neighbors().get(&prev_pk)
            .expect("Previous neighbor not present");
        let next_neighbor = self.state.get_neighbors().get(&next_pk)
            .expect("Next neighbor not present");


        let total_trust = self.state.get_total_trust();
        let prev_trust = prev_neighbor.get_trust();
        let forward_trust = next_neighbor.get_trust();

        let two_pow_64 = BigUint::new(vec![0x1, 0x0, 0x0]);
        let numerator = (two_pow_64 * forward_trust) / (total_trust - &prev_trust);
        let usable_ratio = match numerator.to_u64() {
            Some(num) => Ratio::Numerator(num),
            None => Ratio::One,
        };

        let shared_credits = prev_trust.to_u64().unwrap_or(u64::max_value());

        // Add our freeze link
        request_send_msg.freeze_links.push(NetworkerFreezeLink {
            shared_credits,
            usable_ratio,
        });

        // Queue message to the relevant neighbor. Later this message will be queued to a specific
        // available token channel:
        let neighbor_mutation = NeighborMutation::PushBackPendingRequest(request_send_msg.clone());
        let messenger_mutation = MessengerMutation::NeighborMutation((next_pk.clone(), neighbor_mutation));
        self.apply_mutation(messenger_mutation);
    }

    #[async]
    fn handle_request_send_msg(mut self, 
                               remote_public_key: PublicKey,
                               channel_index: u16,
                               request_send_msg: RequestSendMessage) -> Result<Self, HandleNeighborError> {

        self.cache.freeze_guard.add_frozen_credit(&request_send_msg.create_pending_request().unwrap());
        // TODO: Add rest of add/sub_frozen_credit

        // Find ourselves on the route. If we are not there, abort.
        let pk_pair = request_send_msg.route.find_pk_pair(
            &remote_public_key, 
            self.state.get_local_public_key())
            .expect("Could not find pair in request_send_msg route!");

        let index = match pk_pair {
            PkPairPosition::Dest => {
                self.punt_request_to_crypter(request_send_msg);
                return Ok(self);
            }
            PkPairPosition::NotDest(i) => {
                i.checked_add(1).expect("Route too long!")
            }
        };



        // The node on the route has to be one of our neighbors:
        let next_index = index.checked_add(1).expect("Route too long!");
        let next_public_key = request_send_msg.route.pk_by_index(next_index)
            .expect("index out of range!");
        let mut fself = if !self.state.get_neighbors().contains_key(next_public_key) {
            await!(self.reply_with_failure(remote_public_key.clone(), 
                                           channel_index,
                                           request_send_msg.clone()))?
        } else {
            self
        };


        // Perform DoS protection check:
        Ok(match fself.cache.freeze_guard.verify_freezing_links(&request_send_msg) {
            Some(()) => {
                // Add our freezing link, and queue message to the next node.
                fself.forward_request(request_send_msg);
                fself
            },
            None => {
                // Queue a failure message to this token channel:
                await!(fself.reply_with_failure(remote_public_key, 
                                               channel_index,
                                               request_send_msg))?
            },
        })
    }

    fn handle_response_send_msg(&mut self, 
                               remote_public_key: &PublicKey,
                               channel_index: u16,
                               response_send_msg: ResponseSendMessage,
                               pending_request: PendingNeighborRequest) {

        self.cache.freeze_guard.sub_frozen_credit(&pending_request);
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
                let slot_mutation = SlotMutation::PushBackPendingOperation(response_op);
                let neighbor_mutation = NeighborMutation::SlotMutation((channel_index, slot_mutation));
                let messenger_mutation = MessengerMutation::NeighborMutation((neighbor_public_key, neighbor_mutation));
                self.apply_mutation(messenger_mutation);
            },
        }
    }

    #[async]
    fn handle_failure_send_msg(mut self, 
                               remote_public_key: &PublicKey,
                               channel_index: u16,
                               failure_send_msg: FailureSendMessage,
                               pending_request: PendingNeighborRequest)
                                -> Result<Self, HandleNeighborError> {

        self.cache.freeze_guard.sub_frozen_credit(&pending_request);
        let fself = match self.find_request_origin(&failure_send_msg.request_id) {
            None => {
                // We are the origin of this request, and we got a failure
                // We should pass it back to crypter.
                self.messenger_tasks.push(
                    MessengerTask::CrypterMessage(
                        CrypterMessage::FailureReceived(FailureReceived {
                            request_id: failure_send_msg.request_id,
                            reporting_public_key: failure_send_msg.reporting_public_key,
                        })
                    )
                );
                self
            },
            Some((neighbor_public_key, channel_index)) => {
                let (mut fself, failure_send_msg) = await!(self.failure_message_add_signature(failure_send_msg, 
                                                               pending_request))?;
                // Queue this failure message to another token channel:
                let failure_op = NeighborTcOp::FailureSendMessage(failure_send_msg);
                let slot_mutation = SlotMutation::PushBackPendingOperation(failure_op);
                let neighbor_mutation = NeighborMutation::SlotMutation((channel_index, slot_mutation));
                let messenger_mutation = MessengerMutation::NeighborMutation((neighbor_public_key.clone(), neighbor_mutation));
                fself.apply_mutation(messenger_mutation);

                fself
            },
        };
        Ok(fself)
    }

    /// Process valid incoming operations from remote side.
    #[async]
    fn handle_move_token_output(mut self, 
                                remote_public_key: PublicKey,
                                channel_index: u16,
                                incoming_messages: Vec<IncomingMessage> )
                        -> Result<Self, HandleNeighborError> {

        let mut fself = self;
        for incoming_message in incoming_messages {
            fself = match incoming_message {
                IncomingMessage::Request(request_send_msg) => 
                    await!(fself.handle_request_send_msg(remote_public_key.clone(), channel_index, 
                                                 request_send_msg))?,
                IncomingMessage::Response(IncomingResponseSendMessage {
                                                pending_request, incoming_response}) => {
                    fself.handle_response_send_msg(&remote_public_key, channel_index, 
                                                  incoming_response, pending_request);
                    fself
                },
                IncomingMessage::Failure(IncomingFailureSendMessage {
                                                pending_request, incoming_failure}) => {
                    await!(fself.handle_failure_send_msg(&remote_public_key, channel_index, 
                                                 incoming_failure, pending_request))?
                },
            }
        }
        Ok(fself)
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


        // Clear current incoming inconsistency messages:
        let slot_mutation = SlotMutation::SetIncomingInconsistency(IncomingInconsistency::Empty);
        let neighbor_mutation = NeighborMutation::SlotMutation((channel_index, slot_mutation));
        let messenger_mutation = MessengerMutation::NeighborMutation((remote_public_key.clone(), neighbor_mutation));
        self.apply_mutation(messenger_mutation);


        let token_channel_slot = self.get_token_channel_slot(&remote_public_key, 
                                                              channel_index);
        // Send an InconsistencyError message to remote side:
        let current_token = token_channel_slot.directional
            .calc_channel_reset_token(channel_index);
        let balance_for_reset = token_channel_slot.directional
            .balance_for_reset();

        let inconsistency_error = NeighborInconsistencyError {
            opt_ack: None,
            token_channel_index: channel_index,
            current_token,
            balance_for_reset,
        };

        self.messenger_tasks.push(
            MessengerTask::NeighborMessage(
                NeighborMessage::InconsistencyError(inconsistency_error)));

        // Keep outgoing InconsistencyError message details in memory:
        let slot_mutation = SlotMutation::SetOutgoingInconsistency(OutgoingInconsistency::Sent);
        let neighbor_mutation = NeighborMutation::SlotMutation((channel_index, slot_mutation));
        let messenger_mutation = MessengerMutation::NeighborMutation((remote_public_key.clone(), neighbor_mutation));
        self.apply_mutation(messenger_mutation);
    }


    /// Queue as many messages as possible into available token channel.
    fn queue_outgoing_operations(&mut self,
                           remote_public_key: &PublicKey,
                           channel_index: u16,
                           out_tc: &mut OutgoingTokenChannel) -> Result<(), QueueOperationFailure> {

        let tc_slot = self.get_token_channel_slot(&remote_public_key, 
                                                    channel_index);

        // Set remote_max_debt if needed:
        let remote_max_debt = tc_slot
            .directional
            .remote_max_debt();

        if tc_slot.wanted_remote_max_debt != remote_max_debt {
            out_tc.queue_operation(NeighborTcOp::SetRemoteMaxDebt(tc_slot.wanted_remote_max_debt))?;
        }

        // Set local_send_price if needed:
        let local_send_price = &tc_slot
            .directional
            .token_channel
            .state()
            .send_price
            .local_send_price;

        if tc_slot.wanted_local_send_price != *local_send_price {
            match &tc_slot.wanted_local_send_price {
                Some(wanted_local_send_price) => out_tc.queue_operation(NeighborTcOp::EnableRequests(
                    wanted_local_send_price.clone()))?,
                None => out_tc.queue_operation(NeighborTcOp::DisableRequests)?,
            };
        }

        // Send pending operations (responses and failures)
        // TODO: Possibly replace this clone with something more efficient later:
        let mut pending_operations = tc_slot.pending_operations.clone();
        while let Some(pending_operation) = pending_operations.pop_front() {
            out_tc.queue_operation(pending_operation)?;
            let slot_mutation = SlotMutation::PopFrontPendingOperation;
            let neighbor_mutation = NeighborMutation::SlotMutation((channel_index, slot_mutation));
            let messenger_mutation = MessengerMutation::NeighborMutation((remote_public_key.clone(), neighbor_mutation));
            self.apply_mutation(messenger_mutation);
        }

        // Send requests:
        let neighbor = self.state.get_neighbors().get(&remote_public_key).unwrap();

        let mut pending_requests = neighbor.pending_requests.clone();
        while let Some(pending_request) = pending_requests.pop_front() {
            out_tc.queue_operation(NeighborTcOp::RequestSendMessage(pending_request))?;
            let neighbor_mutation = NeighborMutation::PopFrontPendingRequest;
            let messenger_mutation = MessengerMutation::NeighborMutation((remote_public_key.clone(), neighbor_mutation));
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
                                  channel_index: u16,
                                  received_empty: bool) {

        let token_channel_slot = self.get_token_channel_slot(remote_public_key, channel_index);
        let mut out_tc = token_channel_slot.directional
            .begin_outgoing_move_token(MAX_MOVE_TOKEN_LENGTH).unwrap();

        let res = self.queue_outgoing_operations(remote_public_key,
                                       channel_index,
                                       &mut out_tc);

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
            let slot_mutation = SlotMutation::DirectionalMutation(directional_mutation);
            let neighbor_mutation = NeighborMutation::SlotMutation((channel_index, slot_mutation));
            let messenger_mutation = MessengerMutation::NeighborMutation((remote_public_key.clone(), neighbor_mutation));
            self.apply_mutation(messenger_mutation);
        }
        let token_channel_slot = self.get_token_channel_slot(remote_public_key, channel_index);

        let rand_nonce = RandValue::new(&*self.rng);
        let neighbor_move_token_inner = NeighborMoveTokenInner {
            operations,
            old_token: token_channel_slot.directional.new_token.clone(),
            rand_nonce,
        };

        let directional_mutation = DirectionalMutation::SetDirection(
            MoveTokenDirection::Outgoing(neighbor_move_token_inner));
        let slot_mutation = SlotMutation::DirectionalMutation(directional_mutation);
        let neighbor_mutation = NeighborMutation::SlotMutation((channel_index, slot_mutation));
        let messenger_mutation = MessengerMutation::NeighborMutation((remote_public_key.clone(), neighbor_mutation));
        self.apply_mutation(messenger_mutation);

        let token_channel_slot = self.get_token_channel_slot(remote_public_key, channel_index);
        let outgoing_move_token = token_channel_slot.directional.get_outgoing_move_token().unwrap();

        // Add a task for sending the outgoing move token:
        self.add_task(
            MessengerTask::NeighborMessage(
                NeighborMessage::MoveToken(outgoing_move_token)));
    }

    /// Initiate loading funds (Using a funder message) for token channel, if needed.
    fn initiate_load_funds(&mut self,
                           remote_public_key: &PublicKey,
                           channel_index: u16) {

        let tc_slot = self.get_token_channel_slot(
            remote_public_key, channel_index);

        if tc_slot.opt_pending_send_funds_id.is_some() {
            // We already have a pending payment. Nothing to do here.
            return;
        }
        
        //
        //   -LocalMaxDebt        0  balance    RemoteMaxDebt 
        //  -----[----------------|---|-----------]--------->
        //

        let tc_balance = &tc_slot.directional.token_channel.state().balance;
        let span = tc_balance.remote_max_debt + tc_balance.local_max_debt;

        let left_dist: u64 = u64::try_from(tc_balance.balance.checked_add_unsigned(tc_balance.local_max_debt)
            .unwrap()).unwrap();
        let left_quad = left_dist < span / 4;

        if !left_quad {
            // We are not in the left quadrant. Nothing to do here
            return;
        }

        // We want to bring balance to the middle between -LocalMaxDebt and RemoteMaxDebt
        // dest_balance = (-local_max_debt + remote_max_debt) / 2
        let dest_balance = (0i64).checked_sub_unsigned(tc_balance.local_max_debt).unwrap()
            .checked_add_unsigned(tc_balance.remote_max_debt).unwrap() / 2;
        // payment = dest_balance - balance
        let payment = u64::try_from(dest_balance.checked_sub(tc_balance.balance).unwrap()).unwrap();

        // Create a RequestSendFunds, and push it as a task.
        let payment_id = Uid::new(&*self.rng);

        let send_payment = SendPayment {
            neighbor_public_key: remote_public_key.clone(),
            channel_index,
            payment_id,
            payment,
        };

        self.add_task(MessengerTask::SendPayment(send_payment));

        // Keep the request_id of the generated RequestSendFunds, and save it inside the state:
        let slot_mutation = SlotMutation::SetPendingSendFundsId(Some(payment_id));
        let neighbor_mutation = NeighborMutation::SlotMutation((channel_index, slot_mutation));
        let messenger_mutation = MessengerMutation::NeighborMutation((remote_public_key.clone(), neighbor_mutation));
        self.apply_mutation(messenger_mutation);
    }

    /// Clear all pending inconsistency errors if exist
    fn clear_inconsistency_status(&mut self,
                               remote_public_key: &PublicKey,
                               channel_index: u16) {
        let tc_slot = self.get_token_channel_slot(&remote_public_key, channel_index);
        match tc_slot.inconsistency_status.incoming {
            IncomingInconsistency::Empty => {},
            _ => {
                let slot_mutation = SlotMutation::SetIncomingInconsistency(IncomingInconsistency::Empty);
                let neighbor_mutation = NeighborMutation::SlotMutation((channel_index, slot_mutation));
                let messenger_mutation = MessengerMutation::NeighborMutation((remote_public_key.clone(), neighbor_mutation));
                self.apply_mutation(messenger_mutation);
            },
        }

        let tc_slot = self.get_token_channel_slot(&remote_public_key, channel_index);
        match tc_slot.inconsistency_status.outgoing {
            OutgoingInconsistency::Empty => {},
            _ => {
                let slot_mutation = SlotMutation::SetOutgoingInconsistency(OutgoingInconsistency::Empty);
                let neighbor_mutation = NeighborMutation::SlotMutation((channel_index, slot_mutation));
                let messenger_mutation = MessengerMutation::NeighborMutation((remote_public_key.clone(), neighbor_mutation));
                self.apply_mutation(messenger_mutation);
            },
        }
    }

    /// Handle success with incoming move token.
    #[async]
    fn handle_move_token_success(mut self,
                               remote_public_key: PublicKey,
                               channel_index: u16,
                               receive_move_token_output: ReceiveMoveTokenOutput,
                               is_empty: bool) -> Result<Self, HandleNeighborError> {

        self.clear_inconsistency_status(&remote_public_key,
                                        channel_index);

        match receive_move_token_output {
            ReceiveMoveTokenOutput::Duplicate => Ok(self),
            ReceiveMoveTokenOutput::RetransmitOutgoing(outgoing_move_token) => {
                // Retransmit last sent token channel message:
                self.messenger_tasks.push(
                    MessengerTask::NeighborMessage(
                        NeighborMessage::MoveToken(outgoing_move_token)));
                Ok(self)
            },
            ReceiveMoveTokenOutput::Received(move_token_received) => {

                let MoveTokenReceived {incoming_messages, mutations} = 
                    move_token_received;

                // Apply all mutations:
                for directional_mutation in mutations {
                    let slot_mutation = SlotMutation::DirectionalMutation(directional_mutation);
                    let neighbor_mutation = NeighborMutation::SlotMutation((channel_index, slot_mutation));
                    let messenger_mutation = MessengerMutation::NeighborMutation((remote_public_key.clone(), neighbor_mutation));
                    self.apply_mutation(messenger_mutation);
                }


                let mut fself = await!(self.handle_move_token_output(remote_public_key.clone(),
                                               channel_index,
                                               incoming_messages))?;
                fself.send_through_token_channel(&remote_public_key,
                                                 channel_index,
                                                 is_empty);
                fself.initiate_load_funds(&remote_public_key,
                                          channel_index);
                Ok(fself)
            },
        }
    }


    #[async]
    fn handle_move_token(mut self, 
                         remote_public_key: PublicKey,
                         neighbor_move_token: NeighborMoveToken) -> Result<Self,HandleNeighborError> {

        // Find neighbor:
        let neighbor = match self.state.get_neighbors().get(&remote_public_key) {
            Some(neighbor) => neighbor,
            None => return Ok(self),
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
            return Ok(self)
        }

        let token_channel_slot = self.get_token_channel_slot(&remote_public_key, 
                                                             channel_index);

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
                                           channel_index, 
                                           neighbor_move_token.new_token.clone()))?;

        let token_channel_slot = fself.get_token_channel_slot(&remote_public_key, 
                                                             channel_index);


        let is_empty = neighbor_move_token.operations.is_empty();

        // TODO: Possibly refactor this part into a function?
        let neighbor_move_token_inner = NeighborMoveTokenInner {
            operations: neighbor_move_token.operations,
            old_token: neighbor_move_token.old_token,
            rand_nonce: neighbor_move_token.rand_nonce,
        };
        let receive_move_token_res = token_channel_slot.directional.simulate_receive_move_token(
            neighbor_move_token_inner,
            neighbor_move_token.new_token);

        Ok(match receive_move_token_res {
            Ok(receive_move_token_output) => {
                await!(fself.handle_move_token_success(remote_public_key.clone(),
                                             channel_index,
                                             receive_move_token_output,
                                             is_empty))?
            },
            Err(receive_move_token_error) => {
                fself.handle_move_token_error(&remote_public_key,
                                             channel_index,
                                             receive_move_token_error);
                fself
            },
        })
    }

    fn handle_inconsistency_error(&mut self, 
                                  remote_public_key: &PublicKey,
                                  neighbor_inconsistency_error: NeighborInconsistencyError) {
        
        // Save incoming inconsistency details:
        let token_channel_index = neighbor_inconsistency_error.token_channel_index;
        let incoming = IncomingInconsistency::Incoming(ResetTerms {
            current_token: neighbor_inconsistency_error.current_token.clone(),
            balance_for_reset: neighbor_inconsistency_error.balance_for_reset,
        });

        let slot_mutation = SlotMutation::SetIncomingInconsistency(incoming);
        let neighbor_mutation = NeighborMutation::SlotMutation((token_channel_index, slot_mutation));
        let messenger_mutation = MessengerMutation::NeighborMutation((remote_public_key.clone(), neighbor_mutation));
        self.apply_mutation(messenger_mutation);

        // Obtain information about our reset terms:
        let tc_slot = self.get_token_channel_slot(
            remote_public_key, token_channel_index);
        let directional = &tc_slot.directional;
        let reset_token = directional.calc_channel_reset_token(token_channel_index);
        let balance_for_reset = directional.balance_for_reset();


        // Check if we should send an outgoing inconsistency message:
        let should_send_outgoing = match tc_slot.inconsistency_status.outgoing {
            OutgoingInconsistency::Empty => {
                let slot_mutation = SlotMutation::SetOutgoingInconsistency(OutgoingInconsistency::Sent);
                let neighbor_mutation = NeighborMutation::SlotMutation((token_channel_index, slot_mutation));
                let messenger_mutation = MessengerMutation::NeighborMutation((remote_public_key.clone(), neighbor_mutation));
                self.apply_mutation(messenger_mutation);
                true
            },
            OutgoingInconsistency::Sent => {
                let is_ack_valid = match neighbor_inconsistency_error.opt_ack {
                    Some(acked_reset_token) => acked_reset_token == reset_token,
                    None => false,
                };
                if is_ack_valid {
                    let slot_mutation = SlotMutation::SetOutgoingInconsistency(OutgoingInconsistency::Acked);
                    let neighbor_mutation = NeighborMutation::SlotMutation((token_channel_index, slot_mutation));
                    let messenger_mutation = MessengerMutation::NeighborMutation((remote_public_key.clone(), neighbor_mutation));
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
            let inconsistency_error = NeighborInconsistencyError {
                opt_ack: Some(neighbor_inconsistency_error.current_token.clone()),
                token_channel_index,
                current_token: reset_token.clone(),
                balance_for_reset,
            };

            self.add_task(
                MessengerTask::NeighborMessage(
                    NeighborMessage::InconsistencyError(inconsistency_error)));
        }

    }

    fn handle_set_max_token_channels(&mut self, 
                                     remote_public_key: PublicKey,
                                     neighbor_set_max_token_channels: NeighborSetMaxTokenChannels) {

        let neighbor_mutation = NeighborMutation::SetRemoteMaxChannels(
            neighbor_set_max_token_channels.max_token_channels);
        let messenger_mutation = MessengerMutation::NeighborMutation((remote_public_key.clone(), neighbor_mutation));
        self.apply_mutation(messenger_mutation);
    }

    #[async]
    pub fn handle_neighbor_message(mut self, 
                                   remote_public_key: PublicKey, 
                                   neighbor_message: IncomingNeighborMessage)
                                        -> Result<Self, HandleNeighborError> {
        match neighbor_message {
            IncomingNeighborMessage::MoveToken(neighbor_move_token) =>
                await!(self.handle_move_token(remote_public_key, neighbor_move_token)),
            IncomingNeighborMessage::InconsistencyError(neighbor_inconsistency_error) => {
                self.handle_inconsistency_error(&remote_public_key, neighbor_inconsistency_error);
                Ok(self)
            }
            IncomingNeighborMessage::SetMaxTokenChannels(neighbor_set_max_token_channels) => {
                self.handle_set_max_token_channels(remote_public_key, neighbor_set_max_token_channels);
                Ok(self)
            }
        }
    }

}

