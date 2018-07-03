#![allow(unused)]
use ring::rand::SecureRandom;

use super::messenger_state::{MessengerState, NeighborState, 
    TokenChannelSlot, StateMutateMessage, MessengerStateError};
use super::messenger_handler::{MessengerHandler, MessengerTask};
use app_manager::messages::{NetworkerConfig, AddNeighbor, 
    RemoveNeighbor, SetNeighborStatus, SetNeighborRemoteMaxDebt,
    ResetNeighborChannel, SetNeighborMaxChannels};

/*
pub enum HandleAppManagerError {
    NeighborDoesNotExist,
    TokenChannelDoesNotExist,
    NeighborAlreadyExists,
    MessengerStateError(MessengerStateError),
}
*/

#[allow(unused)]
impl<R: SecureRandom> MessengerHandler<R> {
    fn app_manager_set_neighbor_remote_max_debt(&mut self, 
                                                set_neighbor_remote_max_debt: SetNeighborRemoteMaxDebt) 
        -> (Vec<StateMutateMessage>, Vec<MessengerTask>) {


        let sm_msg = StateMutateMessage::SetNeighborRemoteMaxDebt(set_neighbor_remote_max_debt.clone());
        match self.state.set_neighbor_remote_max_debt(set_neighbor_remote_max_debt) {
            Ok(()) => (vec![sm_msg], vec![]),
            Err(_) => (vec![], vec![]),
        }
    }

    fn app_manager_reset_neighbor_channel(&mut self, 
                                          reset_neighbor_channel: ResetNeighborChannel) 
        -> (Vec<StateMutateMessage>, Vec<MessengerTask>) {

        let sm_msg = StateMutateMessage::ResetNeighborChannel(reset_neighbor_channel.clone());
        match self.state.reset_neighbor_channel(reset_neighbor_channel) {
            Ok(()) => (vec![sm_msg], vec![]),
            Err(_) => (vec![], vec![]),
        }
    }

    fn app_manager_set_neighbor_max_channels(&mut self, 
                                          set_neighbor_max_channels: SetNeighborMaxChannels) 
        -> (Vec<StateMutateMessage>, Vec<MessengerTask>) {

        let sm_msg = StateMutateMessage::SetNeighborMaxChannels(set_neighbor_max_channels.clone());
        match self.state.set_neighbor_max_channels(set_neighbor_max_channels) {
            Ok(()) => (vec![sm_msg], vec![]),
            Err(_) => (vec![], vec![]),
        }
    }

    fn app_manager_add_neighbor(&mut self, 
                                add_neighbor: AddNeighbor) 
        -> (Vec<StateMutateMessage>, Vec<MessengerTask>) {

        let sm_msg = StateMutateMessage::AddNeighbor(add_neighbor.clone());
        match self.state.add_neighbor(add_neighbor) {
            Ok(()) => (vec![sm_msg], vec![]),
            Err(_) => (vec![], vec![]),
        }
    }

    fn app_manager_remove_neighbor(&mut self, remove_neighbor: RemoveNeighbor) 
        -> (Vec<StateMutateMessage>, Vec<MessengerTask>) {

        let sm_msg = StateMutateMessage::RemoveNeighbor(remove_neighbor.clone());
        match self.state.remove_neighbor(remove_neighbor) {
            Ok(()) => (vec![sm_msg], vec![]),
            Err(_) => (vec![], vec![]),
        }
    }


    fn app_manager_set_neighbor_status(&mut self, set_neighbor_status: SetNeighborStatus) 
        -> (Vec<StateMutateMessage>, Vec<MessengerTask>) {

        let sm_msg = StateMutateMessage::SetNeighborStatus(set_neighbor_status.clone());
        match self.state.set_neighbor_status(set_neighbor_status) {
            Ok(()) => (vec![sm_msg], vec![]),
            Err(_) => (vec![], vec![]),
        }
    }

    pub fn handle_app_manager_message(&mut self, 
                                      networker_config: NetworkerConfig) 
        -> (Vec<StateMutateMessage>, Vec<MessengerTask>) {


        match networker_config {
            NetworkerConfig::SetNeighborRemoteMaxDebt(set_neighbor_remote_max_debt) => 
                self.app_manager_set_neighbor_remote_max_debt(set_neighbor_remote_max_debt),
            NetworkerConfig::ResetNeighborChannel(reset_neighbor_channel) => 
                self.app_manager_reset_neighbor_channel(reset_neighbor_channel),
            NetworkerConfig::SetNeighborMaxChannels(set_neighbor_max_channels) => 
                self.app_manager_set_neighbor_max_channels(set_neighbor_max_channels),
            NetworkerConfig::AddNeighbor(add_neighbor) => 
                self.app_manager_add_neighbor(add_neighbor),
            NetworkerConfig::RemoveNeighbor(remove_neighbor) => 
                self.app_manager_remove_neighbor(remove_neighbor),
            NetworkerConfig::SetNeighborStatus(set_neighbor_status) => 
                self.app_manager_set_neighbor_status(set_neighbor_status),
        }
    }

}
