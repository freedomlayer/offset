use super::messenger_state::MessengerState;
use super::handle_neighbor::{NeighborMoveToken, NeighborInconsistencyError, 
    NeighborSetMaxTokenChannels};

pub enum AppManagerMessage {

}

pub enum FunderMessage {

}


#[allow(unused)]
pub enum NeighborMessage {
    MoveToken(NeighborMoveToken),
    InconsistencyError(NeighborInconsistencyError),
    SetMaxTokenChannels(NeighborSetMaxTokenChannels),
}

pub enum CrypterMessage {

}


#[allow(unused)]
pub enum MessengerTask {
    AppManagerMessage(AppManagerMessage),
    FunderMessage(FunderMessage),
    NeighborMessage(NeighborMessage),
    CrypterMessage(CrypterMessage),
}

#[allow(unused)]
pub struct MessengerHandler {
    pub state: MessengerState,
}

impl MessengerHandler {
    #[allow(unused)]
    pub fn handle_timer_tick(&mut self) -> Vec<MessengerTask> {
        // TODO
        unreachable!();
    }
}
