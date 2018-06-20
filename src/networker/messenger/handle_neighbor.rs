use crypto::rand_values::RandValue;
use crypto::identity::PublicKey;
use crypto::uid::Uid;
use proto::networker::ChannelToken;

use super::messenger_handler::{MessengerHandler, MessengerTask, NeighborMessage};
use super::types::{NeighborTcOp, PendingNeighborRequest, FailureSendMessage};
use super::messenger_state::{NeighborState, StateMutateMessage, 
    MessengerStateError, TokenChannelStatus, TokenChannelSlot,
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


pub enum HandleNeighborMessageError {
    NeighborNotFound,
    ChannelIsInconsistent,
    MessengerStateError(MessengerStateError),
}


#[allow(unused)]
impl MessengerHandler {

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
    fn find_request_origin(&self, pending_local_request: &PendingNeighborRequest) -> Option<(PublicKey, u16)> {

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

    fn handle_move_token(&mut self, 
                         remote_public_key: &PublicKey,
                         neighbor_move_token: NeighborMoveToken) 
         -> Result<(Vec<StateMutateMessage>, Vec<MessengerTask>), HandleNeighborMessageError> {

        // Find neighbor:
        let neighbor = self.state.get_neighbors().get(remote_public_key)
            .ok_or(HandleNeighborMessageError::NeighborNotFound)?;


        let channel_index = neighbor_move_token.token_channel_index;
        if channel_index >= neighbor.local_max_channels {
            // Tell remote side that we don't support such a high token channel index:
            let messenger_tasks = vec!(
                MessengerTask::NeighborMessage(
                    NeighborMessage::SetMaxTokenChannels(
                        NeighborSetMaxTokenChannels {
                            max_token_channels: neighbor.local_max_channels,
                        }
                    )
                )
            );
            return Ok((Vec::new(), messenger_tasks));
        }

        let mut db_messages = Vec::new();

        if !neighbor.token_channel_slots.contains_key(&channel_index) {
            let mut new_db_messages = self.state.init_token_channel(SmInitTokenChannel {
                neighbor_public_key: remote_public_key.clone(),
                channel_index,
            }).map_err(|e| HandleNeighborMessageError::MessengerStateError(e))?;
            db_messages.append(&mut new_db_messages);
        }

        let neighbor = self.state.get_neighbors().get(remote_public_key)
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
            return Err(HandleNeighborMessageError::ChannelIsInconsistent);
        };

        // Check if incoming message is an attempt to reset channel.
        // We can know this by checking if new_token is a special value.
        let reset_token = token_channel_slot.tc_state.calc_channel_reset_token(channel_index);
        let balance_for_reset = token_channel_slot.tc_state.balance_for_reset();
        if neighbor_move_token.new_token == reset_token {
            // This is a reset message. We reset the token channel:
            
            // Mark all pending requests to this neighbor as errors.
            // As the token channel is being reset, we can be sure we will never obtain a response
            // for those requests.
            let pending_local_requests = token_channel_slot.tc_state
                .get_token_channel()
                .pending_local_requests();

            let mut entries = Vec::new();
            for (local_request_id, pending_local_request) in pending_local_requests {
                let origin = self.find_request_origin(pending_local_request);
                let (origin_public_key, origin_channel_index) = match origin {
                    Some((public_key, channel_index)) => (public_key, channel_index),
                    None => continue,
                };
                entries.push((origin_public_key.clone(), origin_channel_index, pending_local_request.clone()));
            }

            for (origin_public_key, origin_channel_index, pending_local_request) in entries.into_iter() {
                let failure_op = NeighborTcOp::FailureSendMessage(FailureSendMessage {
                    request_id: pending_local_request.request_id.clone(),
                    reporting_public_key: self.state.get_local_public_key().clone(),
                    rand_nonce_signatures: Vec::new(), // TODO
                });
                self.state.token_channel_push_op(SmTokenChannelPushOp {
                    neighbor_public_key: origin_public_key.clone(),
                    channel_index: origin_channel_index,
                    neighbor_op: failure_op,
                }).map_err(|e| HandleNeighborMessageError::MessengerStateError(e))?;

                unreachable!(); // TODO: construct rand_nonce_signatures
            }

            // TODO: 
            // - Construct rand_nonce_signatures (Possibly require adding a random generator
            //      argument, to generate the rand nonce).
            // - Add database messages for all state mutations (How to do this well?)
            // - Continue processing the MoveToken message.

            // Replace slot with a new one:
            let token_channel_slot = TokenChannelSlot::new_from_reset(self.state.get_local_public_key(),
                                                                        remote_public_key,
                                                                        &reset_token,
                                                                        balance_for_reset);


            // neighbor.token_channel_slots.insert(channel_index, token_channel_slot);
        }




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

    fn handle_inconsistency_error(&mut self, 
                                  remote_public_key: &PublicKey,
                                  neighbor_inconsistency_error: NeighborInconsistencyError)
         -> Result<(Vec<StateMutateMessage>, Vec<MessengerTask>), HandleNeighborMessageError> {
        unreachable!();
    }

    fn handle_set_max_token_channels(&mut self, 
                                     remote_public_key: &PublicKey,
                                     neighbor_set_max_token_channels: NeighborSetMaxTokenChannels)
         -> Result<(Vec<StateMutateMessage>, Vec<MessengerTask>), HandleNeighborMessageError> {
        unreachable!();
    }

    pub fn handle_neighbor_message(&mut self, 
                                   remote_public_key: &PublicKey, 
                                   neighbor_message: IncomingNeighborMessage)
        -> Result<(Vec<StateMutateMessage>, Vec<MessengerTask>), HandleNeighborMessageError> {

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
