#![allow(unused)]

use super::types::NeighborTcOp;
use super::messenger_state::{MessengerState, TokenChannelSlot, MessengerTask, DatabaseMessage};
use app_manager::messages::{NetworkerConfig, AddNeighbor, 
    RemoveNeighbor, SetNeighborStatus, SetNeighborRemoteMaxDebt,
    ResetNeighborChannel, SetNeighborMaxChannels};

pub enum HandleAppManagerError {
    NeighborDoesNotExist,
    TokenChannelDoesNotExist,
    NeighborAlreadyExists,
}

#[allow(unused)]
impl MessengerState {
    fn app_manager_set_neighbor_remote_max_debt(&mut self, 
                                                set_neighbor_remote_max_debt: SetNeighborRemoteMaxDebt) 
        -> Result<(Option<DatabaseMessage>, Vec<MessengerTask>), HandleAppManagerError> {

        // Check if we have the requested neighbor:
        let neighbor_state = self.neighbors.get_mut(&set_neighbor_remote_max_debt.neighbor_public_key)
            .ok_or(HandleAppManagerError::NeighborDoesNotExist)?;
        
        // Find the token channel slot:
        let token_channel_slot = neighbor_state.token_channel_slots.get_mut(&set_neighbor_remote_max_debt.channel_index)
            .ok_or(HandleAppManagerError::TokenChannelDoesNotExist)?;

        token_channel_slot.wanted_remote_max_debt = set_neighbor_remote_max_debt.remote_max_debt;
        Ok((Some(DatabaseMessage::SetNeighborRemoteMaxDebt(set_neighbor_remote_max_debt)), Vec::new()))
    }

    fn app_manager_reset_neighbor_channel(&mut self, 
                                          reset_neighbor_channel: ResetNeighborChannel) 
        -> Result<(Option<DatabaseMessage>, Vec<MessengerTask>), HandleAppManagerError> {


        // Check if we have the requested neighbor:
        let neighbor_state = self.neighbors.get_mut(&reset_neighbor_channel.neighbor_public_key)
            .ok_or(HandleAppManagerError::NeighborDoesNotExist)?;

        let new_token_channel_slot = TokenChannelSlot::new(
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

        unreachable!();

        /*
        // Check if we have the requested neighbor:
        let _ = self.neighbors.get_mut(&set_neighbor_max_channels.neighbor_public_key)
            .ok_or(HandleAppManagerError::NeighborDoesNotExist)?;
        let mut res_tasks = Vec::new();

        // Save in database:
        res_tasks.push(MessengerTask::DatabaseMessage(DatabaseMessage::SetNeighborMaxChannels(
                    set_neighbor_max_channels)));
        // After successfuly saved in database, we will update the value of neighbor_max_channels
        // kept in RAM.
        
        Ok(res_tasks)
        */
    }

    fn app_manager_add_neighbor(&mut self, add_neighbor: AddNeighbor) 
        -> Result<(Option<DatabaseMessage>, Vec<MessengerTask>), HandleAppManagerError> {

        unreachable!();

        /*
        // If we already have the neighbor: return error.
        if self.neighbors.contains_key(&add_neighbor.neighbor_public_key) {
            return Err(HandleAppManagerError::NeighborAlreadyExists);
        }

        // Send message to database to add the neighbor.
        //      After done adding to database, add neighbor to RAM.
        let mut res_tasks = Vec::new();
        res_tasks.push(MessengerTask::DatabaseMessage(DatabaseMessage::AddNeighbor(add_neighbor)));
        Ok(res_tasks)
        */
    }

    fn app_manager_remove_neighbor(&mut self, remove_neighbor: RemoveNeighbor) 
        -> Result<(Option<DatabaseMessage>, Vec<MessengerTask>), HandleAppManagerError> {

        unreachable!();

        /*
        let _ = self.neighbors.get_mut(&remove_neighbor.neighbor_public_key)
            .ok_or(HandleAppManagerError::NeighborDoesNotExist)?;
        let mut res_tasks = Vec::new();
        res_tasks.push(MessengerTask::DatabaseMessage(DatabaseMessage::RemoveNeighbor(remove_neighbor)));
        Ok(res_tasks)
        */
    }

    fn app_manager_set_neighbor_status(&mut self, set_neighbor_status: SetNeighborStatus) 
        -> Result<(Option<DatabaseMessage>, Vec<MessengerTask>), HandleAppManagerError> {

        unreachable!();

        /*
        let _ = self.neighbors.get_mut(&set_neighbor_status.neighbor_public_key)
            .ok_or(HandleAppManagerError::NeighborDoesNotExist)?;
        let mut res_tasks = Vec::new();
        res_tasks.push(MessengerTask::DatabaseMessage(DatabaseMessage::SetNeighborStatus(set_neighbor_status)));
        Ok(res_tasks)
        */
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

}
