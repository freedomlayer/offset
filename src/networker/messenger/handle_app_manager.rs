use crypto::identity::PublicKey;
use super::types::NeighborTcOp;
use super::messenger_state::{MessengerState, NeighborState, MessengerTask, DatabaseMessage};
use app_manager::messages::{NetworkerConfig, AddNeighbor, 
    RemoveNeighbor, SetNeighborStatus,  SetNeighborRemoteMaxDebt,
    ResetNeighborChannel, SetNeighborMaxChannels};

pub enum HandleAppManagerError {
    NeighborDoesNotExist,
    TokenChannelDoesNotExist,
    NeighborAlreadyExists,
}

#[allow(unused)]
impl MessengerState {
    fn get_neighbor_state(&mut self, neighbor_public_key: &PublicKey) -> Result<&mut NeighborState, HandleAppManagerError> {
        // Check if we have the requested neighbor:
        match self.neighbors.get_mut(neighbor_public_key) {
            Some(neighbor_state) => Ok(neighbor_state),
            None => Err(HandleAppManagerError::NeighborDoesNotExist),
        }
    }

    fn app_manager_set_neighbor_remote_max_debt(&mut self, 
                                                set_neighbor_remote_max_debt: SetNeighborRemoteMaxDebt) 
        -> Result<Vec<MessengerTask>, HandleAppManagerError> {

        // Check if we have the requested neighbor:
        let neighbor_state = self.get_neighbor_state(&set_neighbor_remote_max_debt.neighbor_public_key)?;
        
        // Find the token channel slot:
        let token_channel_slot = match neighbor_state.token_channel_slots.get_mut(&set_neighbor_remote_max_debt.channel_index) {
            Some(token_channel_slot) => Ok(token_channel_slot),
            None => Err(HandleAppManagerError::TokenChannelDoesNotExist),
        }?;

        // Add a request to change neighbor max debt to the waiting queue of the token channel:
        token_channel_slot.pending_operations.push(
                NeighborTcOp::SetRemoteMaxDebt(set_neighbor_remote_max_debt.remote_max_debt));
        // After this request is completed, we will update database and finally update the value of
        // remote_max_debt kept in RAM.

        Ok(Vec::new())
    }

    fn app_manager_reset_neighbor_channel(&mut self, 
                                          reset_neighbor_channel: ResetNeighborChannel) 
        -> Result<Vec<MessengerTask>, HandleAppManagerError> {

        unreachable!();
    }

    fn app_manager_set_neighbor_max_channels(&mut self, 
                                          set_neighbor_max_channels: SetNeighborMaxChannels) 
        -> Result<Vec<MessengerTask>, HandleAppManagerError> {

        // Check if we have the requested neighbor:
        let _ = self.get_neighbor_state(&set_neighbor_max_channels.neighbor_public_key)?;
        let mut res_tasks = Vec::new();

        // Save in database:
        res_tasks.push(MessengerTask::DatabaseMessage(DatabaseMessage::SetNeighborMaxChannels(
                    set_neighbor_max_channels)));
        // After successfuly saved in database, we will update the value of neighbor_max_channels
        // kept in RAM.
        
        Ok(res_tasks)
    }

    fn app_manager_add_neighbor(&mut self, add_neighbor: AddNeighbor) -> Result<Vec<MessengerTask>, HandleAppManagerError> {
        // - If we already have the neighbor: return error.
        if self.neighbors.contains_key(&add_neighbor.neighbor_public_key) {
            return Err(HandleAppManagerError::NeighborAlreadyExists);
        }
        // - Send message to database to add the neighbor.
        //      After done adding to database, add neighbor to RAM.
        let mut res_tasks = Vec::new();

        // Save in database:
        res_tasks.push(MessengerTask::DatabaseMessage(DatabaseMessage::AddNeighbor(add_neighbor)));

        Ok(res_tasks)
    }

    fn app_manager_remove_neighbor(&mut self, remove_neighbor: RemoveNeighbor) -> Result<Vec<MessengerTask>, HandleAppManagerError> {
        unreachable!();
    }

    fn app_manager_set_neighbor_status(&mut self, set_neighbor_status: SetNeighborStatus) -> Result<Vec<MessengerTask>, HandleAppManagerError> {
        unreachable!();
    }

    pub fn handle_app_manager_message(&mut self, 
                                      networker_config: NetworkerConfig) -> Result<Vec<MessengerTask>, HandleAppManagerError> {
        // TODO
        
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
        };
        unreachable!();
    }

}
