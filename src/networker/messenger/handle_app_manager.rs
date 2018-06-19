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

        let mut db_messages = Vec::new();

        self.state.set_neighbor_remote_max_debt(&mut db_messages,
                                                &set_neighbor_remote_max_debt.neighbor_public_key,
                                                set_neighbor_remote_max_debt.channel_index,
                                                set_neighbor_remote_max_debt.remote_max_debt)
            .map_err(|e| HandleAppManagerError::MessengerStateError(e))?;

        Ok((db_messages, Vec::new()))
    }

    /*
    fn app_manager_reset_neighbor_channel(&mut self, 
                                          reset_neighbor_channel: ResetNeighborChannel) 
        -> Result<(Option<DatabaseMessage>, Vec<MessengerTask>), HandleAppManagerError> {

        // Check if we have the requested neighbor:
        let neighbor_state = self.neighbors.get_mut(&reset_neighbor_channel.neighbor_public_key)
            .ok_or(HandleAppManagerError::NeighborDoesNotExist)?;

        let new_token_channel_slot = TokenChannelSlot::new_from_reset(
            &self.local_public_key,
            &reset_neighbor_channel.neighbor_public_key,
            &reset_neighbor_channel.current_token,
            reset_neighbor_channel.balance_for_reset);

        // Replace the old token channel slot with the new one:
        if !neighbor_state.token_channel_slots.contains_key(&reset_neighbor_channel.channel_index) {
            return Err(HandleAppManagerError::TokenChannelDoesNotExist);
        }
        
        neighbor_state.token_channel_slots.insert(reset_neighbor_channel.channel_index, 
                                                  new_token_channel_slot);

        Ok((Some(DatabaseMessage::ResetNeighborChannel(reset_neighbor_channel)), 
            Vec::new()))
    }

    fn app_manager_set_neighbor_max_channels(&mut self, 
                                          set_neighbor_max_channels: SetNeighborMaxChannels) 
        -> Result<(Option<DatabaseMessage>, Vec<MessengerTask>), HandleAppManagerError> {

        // Check if we have the requested neighbor:
        let neighbor_state = self.neighbors.get_mut(&set_neighbor_max_channels.neighbor_public_key)
            .ok_or(HandleAppManagerError::NeighborDoesNotExist)?;

        neighbor_state.local_max_channels = set_neighbor_max_channels.max_channels;

        Ok((Some(DatabaseMessage::SetNeighborMaxChannels(set_neighbor_max_channels)), 
            Vec::new()))
    }

    fn app_manager_add_neighbor(&mut self, add_neighbor: AddNeighbor) 
        -> Result<(Option<DatabaseMessage>, Vec<MessengerTask>), HandleAppManagerError> {

        // If we already have the neighbor: return error.
        if self.neighbors.contains_key(&add_neighbor.neighbor_public_key) {
            return Err(HandleAppManagerError::NeighborAlreadyExists);
        }

        // Otherwise, we add a new neighbor:
        let neighbor_state = NeighborState::new(
                add_neighbor.neighbor_socket_addr,
                add_neighbor.max_channels);

        self.neighbors.insert(add_neighbor.neighbor_public_key.clone(), neighbor_state);

        Ok((Some(DatabaseMessage::AddNeighbor(add_neighbor)), 
            Vec::new()))

    }

    fn app_manager_remove_neighbor(&mut self, remove_neighbor: RemoveNeighbor) 
        -> Result<(Option<DatabaseMessage>, Vec<MessengerTask>), HandleAppManagerError> {

        let _ = self.neighbors.remove(&remove_neighbor.neighbor_public_key)
            .ok_or(HandleAppManagerError::NeighborDoesNotExist)?;

        Ok((Some(DatabaseMessage::RemoveNeighbor(remove_neighbor)), 
            Vec::new()))
    }

    fn app_manager_set_neighbor_status(&mut self, set_neighbor_status: SetNeighborStatus) 
        -> Result<(Option<DatabaseMessage>, Vec<MessengerTask>), HandleAppManagerError> {

        // Check if we have the requested neighbor:
        let neighbor_state = self.neighbors.get_mut(&set_neighbor_status.neighbor_public_key)
            .ok_or(HandleAppManagerError::NeighborDoesNotExist)?;

        neighbor_state.status = set_neighbor_status.status;

        Ok((Some(DatabaseMessage::SetNeighborStatus(set_neighbor_status)), 
            Vec::new()))
    }

    pub fn handle_app_manager_message(&mut self, 
                                      networker_config: NetworkerConfig) 
        -> Result<(Option<DatabaseMessage>, Vec<MessengerTask>), HandleAppManagerError> {


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
    */

}
