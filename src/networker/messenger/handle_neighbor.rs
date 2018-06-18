use crypto::rand_values::RandValue;
use crypto::identity::PublicKey;
use proto::networker::ChannelToken;

use super::messenger_state::{MessengerState, MessengerTask, TokenChannelSlot, 
    TokenChannelStatus, DatabaseMessage, NeighborMessage};
use super::types::NeighborTcOp;

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
}


#[allow(unused)]
impl MessengerState {
    fn handle_move_token(&mut self, 
                         remote_public_key: &PublicKey,
                         neighbor_move_token: NeighborMoveToken) 
         -> Result<(Option<DatabaseMessage>, Vec<MessengerTask>), HandleNeighborMessageError> {

        // Find neighbor:
        let neighbor = self.neighbors.get_mut(remote_public_key)
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
            return Ok((None, messenger_tasks));
        }

        
        // Obtain existing token channel slot, or create a new one:
        let token_channel_slot = neighbor.token_channel_slots
            .entry(channel_index)
            .or_insert(TokenChannelSlot::new(&self.local_public_key,
                                             &remote_public_key,
                                             channel_index));

        // Check if the channel is inconsistent.
        // This means that the remote side has sent an InconsistencyError message in the past.
        // In this case, we are not willing to accept new messages from the remote side until the
        // inconsistency is resolved.
        if let TokenChannelStatus::Inconsistent { .. } 
                    = token_channel_slot.tc_status {
            return Err(HandleNeighborMessageError::ChannelIsInconsistent);
        };

        // TODO:
        // Check if incoming message is an attempt to reset channel.
        // We can know this by checking if new_token is a special value.
        
        // let reset_new_token = 

        // if neighbor_move_token.new_token == 



        // TODO:
        // - Attempt to receieve the neighbor_move_token transaction.
        //      - On failure: Report inconsistency to AppManager
        //      - On success: 
        //          - Ignore? (If duplicate)
        //          - Retransmit outgoing?
        //          - Handle incoming messages
        unreachable!();
    }

    fn handle_inconsistency_error(&mut self, 
                                  remote_public_key: &PublicKey,
                                  neighbor_inconsistency_error: NeighborInconsistencyError)
         -> Result<(Option<DatabaseMessage>, Vec<MessengerTask>), HandleNeighborMessageError> {
        unreachable!();
    }

    fn handle_set_max_token_channels(&mut self, 
                                     remote_public_key: &PublicKey,
                                     neighbor_set_max_token_channels: NeighborSetMaxTokenChannels)
         -> Result<(Option<DatabaseMessage>, Vec<MessengerTask>), HandleNeighborMessageError> {
        unreachable!();
    }

    pub fn handle_neighbor_message(&mut self, 
                                   remote_public_key: &PublicKey, 
                                   neighbor_message: IncomingNeighborMessage)
        -> Result<(Option<DatabaseMessage>, Vec<MessengerTask>), HandleNeighborMessageError> {

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
