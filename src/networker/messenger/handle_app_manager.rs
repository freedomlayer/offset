#![allow(unused)]

use super::messenger_state::{MessengerState, NeighborState, 
    TokenChannelSlot, DatabaseMessage, MessengerStateError};
use super::messenger_handler::{MessengerHandler, MessengerTask};
use app_manager::messages::{NetworkerConfig, AddNeighbor, 
    RemoveNeighbor, SetNeighborStatus, SetNeighborRemoteMaxDebt,
    ResetNeighborChannel, SetNeighborMaxChannels};

#[allow(unused)]
pub enum HandleAppManagerError {
    NeighborDoesNotExist,
    TokenChannelDoesNotExist,
    NeighborAlreadyExists,
    MessengerStateError(MessengerStateError),
}

#[allow(unused)]
impl MessengerHandler {
    fn app_manager_set_neighbor_remote_max_debt(&mut self, 
                                                set_neighbor_remote_max_debt: SetNeighborRemoteMaxDebt) 
        -> Result<(Vec<DatabaseMessage>, Vec<MessengerTask>), HandleAppManagerError> {

        let db_messages = self.state.set_neighbor_remote_max_debt(set_neighbor_remote_max_debt)
            .map_err(|e| HandleAppManagerError::MessengerStateError(e))?;

        Ok((db_messages, Vec::new()))
    }

    fn app_manager_reset_neighbor_channel(&mut self, 
                                          reset_neighbor_channel: ResetNeighborChannel) 
        -> Result<(Vec<DatabaseMessage>, Vec<MessengerTask>), HandleAppManagerError> {

        let db_messages = self.state.reset_neighbor_channel(reset_neighbor_channel)
            .map_err(|e| HandleAppManagerError::MessengerStateError(e))?;

        Ok((db_messages, Vec::new()))
    }

    fn app_manager_set_neighbor_max_channels(&mut self, 
                                          set_neighbor_max_channels: SetNeighborMaxChannels) 
        -> Result<(Vec<DatabaseMessage>, Vec<MessengerTask>), HandleAppManagerError> {

        let db_messages = self.state.set_neighbor_max_channels(set_neighbor_max_channels)
            .map_err(|e| HandleAppManagerError::MessengerStateError(e))?;

        Ok((db_messages, Vec::new()))
    }

    fn app_manager_add_neighbor(&mut self, 
                                add_neighbor: AddNeighbor) 
        -> Result<(Vec<DatabaseMessage>, Vec<MessengerTask>), HandleAppManagerError> {

        let db_messages = self.state.add_neighbor(add_neighbor)
            .map_err(|e| HandleAppManagerError::MessengerStateError(e))?;

        Ok((db_messages, Vec::new()))
    }

    fn app_manager_remove_neighbor(&mut self, remove_neighbor: RemoveNeighbor) 
        -> Result<(Vec<DatabaseMessage>, Vec<MessengerTask>), HandleAppManagerError> {

        let db_messages = self.state.remove_neighbor(remove_neighbor)
            .map_err(|e| HandleAppManagerError::MessengerStateError(e))?;

        Ok((db_messages, Vec::new()))
    }


    fn app_manager_set_neighbor_status(&mut self, set_neighbor_status: SetNeighborStatus) 
        -> Result<(Vec<DatabaseMessage>, Vec<MessengerTask>), HandleAppManagerError> {

        let db_messages = self.state.set_neighbor_status(set_neighbor_status)
            .map_err(|e| HandleAppManagerError::MessengerStateError(e))?;

        Ok((db_messages, Vec::new()))
    }

    pub fn handle_app_manager_message(&mut self, 
                                      networker_config: NetworkerConfig) 
        -> Result<(Vec<DatabaseMessage>, Vec<MessengerTask>), HandleAppManagerError> {


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
