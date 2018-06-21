use futures::{Future, future};

use ring::rand::SecureRandom;

use crypto::rand_values::RandValue;
use crypto::identity::PublicKey;
use crypto::uid::Uid;

use proto::networker::ChannelToken;

use super::messenger_handler::{MessengerHandler, MessengerTask, NeighborMessage};
use super::types::{NeighborTcOp, PendingNeighborRequest, FailureSendMessage};
use super::messenger_state::{NeighborState, StateMutateMessage, 
    /* MessengerStateError, */TokenChannelStatus, TokenChannelSlot,
    SmInitTokenChannel, SmTokenChannelPushOp};

#[allow(unused)]
pub struct NeighborMoveToken {
    token_channel_index: u16,
    operations: Vec<NeighborTcOp>,
    old_token: ChannelToken,
    rand_nonce: RandValue,
    new_token: ChannelToken,
}

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
    fn find_request_origin(&self, pending_local_request: &PendingNeighborRequest) 
        -> Option<(PublicKey, u16)> {

        let local_index = pending_local_request.route.pk_index(self.state.get_local_public_key())
            .expect("Can not find local public key inside route!");
        let prev_index = local_index.checked_sub(1)?;
        let prev_pk = pending_local_request.route.pk_by_index(prev_index)
            .expect("Index was not found!");
        let orig_neighbor = self.state.get_neighbors()
            .get(&prev_pk)
            .expect("Originator neighbor is missing!");

        let channel_index = self.find_token_channel_by_request_id(&orig_neighbor, 
                                              &pending_local_request.request_id)
                                                .expect("request can not be found!");
        Some((prev_pk.clone(), channel_index))
    }

    fn cancel_local_pending_requests(mut self, 
                                     neighbor_public_key: PublicKey, 
                                     channel_index: u16)
            -> Box<Future<Item=(Self, Vec<StateMutateMessage>), Error=()>> {

        let mut sm_messages = Vec::new();

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
            let origin = self.find_request_origin(pending_local_request);
            let (origin_public_key, origin_channel_index) = match origin {
                Some((public_key, channel_index)) => (public_key, channel_index),
                None => continue,
            };
            requests_to_cancel.push((origin_public_key.clone(), 
                                     origin_channel_index, pending_local_request.clone()));
        }

        // Queue a failure messages for all the pending requests we want to cancel:
        for (origin_public_key, origin_channel_index, 
             pending_local_request) in requests_to_cancel {

            let failure_op = NeighborTcOp::FailureSendMessage(FailureSendMessage {
                request_id: pending_local_request.request_id,
                reporting_public_key: self.state.get_local_public_key().clone(),
                rand_nonce_signatures: Vec::new(), // TODO
            });
            let sm_msg = StateMutateMessage::TokenChannelPushOp(SmTokenChannelPushOp {
                neighbor_public_key: origin_public_key.clone(),
                channel_index: origin_channel_index,
                neighbor_op: failure_op,
            });
            self.state.mutate(sm_msg.clone())
                .expect("Could not push neighbor operation into channel!");
            sm_messages.push(sm_msg);
            unreachable!(); // TODO: construct rand_nonce_signatures
        }

        Box::new(future::ok((self, sm_messages)))
    }

    #[allow(type_complexity)]
    fn handle_move_token(mut self, 
                         remote_public_key: PublicKey,
                         neighbor_move_token: NeighborMoveToken) 
         -> Box<Future<Item=(Self, Vec<StateMutateMessage>, Vec<MessengerTask>), Error=()>> {

        let mut sm_messages = Vec::new();
        let mut messenger_tasks = Vec::new();

        // Find neighbor:
        let neighbor = match self.state.get_neighbors().get(&remote_public_key) {
            Some(neighbor) => neighbor,
            None => return Box::new(future::ok((self, sm_messages, messenger_tasks))),
        };

        let channel_index = neighbor_move_token.token_channel_index;
        if channel_index >= neighbor.local_max_channels {
            // Tell remote side that we don't support such a high token channel index:
            messenger_tasks.push(
                MessengerTask::NeighborMessage(
                    NeighborMessage::SetMaxTokenChannels(
                        NeighborSetMaxTokenChannels {
                            max_token_channels: neighbor.local_max_channels,
                        }
                    )
                )
            );
            return Box::new(future::ok((self, sm_messages, messenger_tasks)));
        }


        if !neighbor.token_channel_slots.contains_key(&channel_index) {
            let sm_msg = StateMutateMessage::InitTokenChannel(SmInitTokenChannel {
                neighbor_public_key: remote_public_key.clone(),
                channel_index,
            });
            self.state.mutate(sm_msg.clone())
                .expect("Failed to initialize token channel!");
            sm_messages.push(sm_msg);
        }

        let neighbor = self.state.get_neighbors().get(&remote_public_key)
            .expect("Neighbor not found!");
        let token_channel_slot = neighbor.token_channel_slots
            .get(&channel_index)
            .expect("token_channel_slot not found!");

        // QUESTION: Should Database be informed about the creation of a new token channel?
        // This is not really a creation of anything new, as we create the default new channel.

        // Check if the channel is inconsistent.
        // This means that the remote side has sent an InconsistencyError message in the past.
        // In this case, we are not willing to accept new messages from the remote side until the
        // inconsistency is resolved.
        if let TokenChannelStatus::Inconsistent { .. } 
                    = token_channel_slot.tc_status {
            return Box::new(future::ok((self, sm_messages, messenger_tasks)));
        };


        // Check if incoming message is an attempt to reset channel.
        // We can know this by checking if new_token is a special value.
        let reset_token = token_channel_slot.tc_state.calc_channel_reset_token(channel_index);
        let balance_for_reset = token_channel_slot.tc_state.balance_for_reset();

        let fself = if neighbor_move_token.new_token == reset_token {
            // This is a reset message. We reset the token channel:
            
            // Mark all pending requests to this neighbor as errors.
            // As the token channel is being reset, we can be sure we will never obtain a response
            // for those requests.

            let fut = self.cancel_local_pending_requests(
                remote_public_key.clone(), channel_index)
            .and_then(|(fself, mut res_sm_messages)| {

                // TODO: 
                // - Construct rand_nonce_signatures (Possibly require adding a random generator
                //      argument, to generate the rand nonce).
                // - Add database messages for all state mutations (How to do this well?)
                // - Continue processing the MoveToken message.

                sm_messages.append(&mut res_sm_messages);

                // Replace slot with a new one:
                let token_channel_slot = TokenChannelSlot::new_from_reset(
                    fself.state.get_local_public_key(),
                    &remote_public_key,
                    &reset_token,
                    balance_for_reset);

                // neighbor.token_channel_slots.insert(channel_index, token_channel_slot);

                Ok(fself)
            });
            let b: Box<Future<Item=Self, Error=()>> = Box::new(fut);
            b
        } else {
            let b: Box<Future<Item=Self, Error=()>> = Box::new(future::ok(self));
            b
        };

        // TODO:
        // - Attempt to receive the neighbor_move_token transaction.
        //      - On failure: Report inconsistency to AppManager
        //      - On success: 
        //          - Ignore? (If duplicate)
        //          - Retransmit outgoing?
        //          - Handle incoming messages
        //
        // - Possibly send any pending messages through this token channel (But first - write to
        //  database).
        unreachable!();
    }

    #[allow(type_complexity)]
    fn handle_inconsistency_error(self, 
                                  remote_public_key: PublicKey,
                                  neighbor_inconsistency_error: NeighborInconsistencyError)
         -> Box<Future<Item=(Self, Vec<StateMutateMessage>, Vec<MessengerTask>), Error=()>> {

        let mut db_messages = Vec::new();
        let mut messenger_tasks = Vec::new();
        unreachable!();
        Box::new(future::ok((self, db_messages, messenger_tasks)))
    }

    #[allow(type_complexity)]
    fn handle_set_max_token_channels(self, 
                                     remote_public_key: PublicKey,
                                     neighbor_set_max_token_channels: NeighborSetMaxTokenChannels)
         -> Box<Future<Item=(Self, Vec<StateMutateMessage>, Vec<MessengerTask>), Error=()>> {
        let mut db_messages = Vec::new();
        let mut messenger_tasks = Vec::new();
        unreachable!();
        Box::new(future::ok((self, db_messages, messenger_tasks)))

    }

    #[allow(type_complexity)]
    pub fn handle_neighbor_message(self, 
                                   remote_public_key: PublicKey, 
                                   neighbor_message: IncomingNeighborMessage)
         -> Box<Future<Item=(Self, Vec<StateMutateMessage>, Vec<MessengerTask>), Error=()>> {

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
