use crypto::rand_values::RandValue;
use crypto::identity::PublicKey;
use proto::networker::ChannelToken;

use super::messenger_state::{MessengerState, MessengerTask, DatabaseMessage};
use super::types::NeighborTcOp;

#[allow(unused)]
pub struct NeighborMoveToken {
    token_channel_index: u16,
    operations: Vec<NeighborTcOp>,
    old_token: ChannelToken,
    rand_nonce: RandValue,
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
pub enum NeighborMessage {
    MoveToken(NeighborMoveToken),
    InconsistencyError(NeighborInconsistencyError),
    SetMaxTokenChannels(NeighborSetMaxTokenChannels),
}


pub enum HandleNeighborMessageError {
}


#[allow(unused)]
impl MessengerState {
    fn handle_move_token(&mut self, 
                         remote_public_key: &PublicKey,
                         neighbor_move_token: NeighborMoveToken) 
         -> Result<(Option<DatabaseMessage>, Vec<MessengerTask>), HandleNeighborMessageError> {
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
                                   neighbor_message: NeighborMessage)
        -> Result<(Option<DatabaseMessage>, Vec<MessengerTask>), HandleNeighborMessageError> {

        match neighbor_message {
            NeighborMessage::MoveToken(neighbor_move_token) =>
                self.handle_move_token(remote_public_key, neighbor_move_token),
            NeighborMessage::InconsistencyError(neighbor_inconsistency_error) =>
                self.handle_inconsistency_error(remote_public_key, neighbor_inconsistency_error),
            NeighborMessage::SetMaxTokenChannels(neighbor_set_max_token_channels) =>
                self.handle_set_max_token_channels(remote_public_key, neighbor_set_max_token_channels),
        }
    }
}
