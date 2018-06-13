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
    max_token_channels: u64,
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
    pub fn handle_channeler_message(&mut self, 
                                    remote_public_key: &PublicKey, 
                                    neighbor_message: NeighborMessage)
        -> Result<(Option<DatabaseMessage>, Vec<MessengerTask>), HandleNeighborMessageError> {

        // TODO
        unreachable!();
    }
}
