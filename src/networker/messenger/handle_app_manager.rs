use super::messenger_state::{MessengerState, MessengerTask};
use app_manager::messages::{NetworkerConfig, AddNeighbor, 
    RemoveNeighbor, SetNeighborStatus,  SetNeighborRemoteMaxDebt,
    ResetNeighborChannel, SetNeighborMaxChannels};

#[allow(unused)]
impl MessengerState {
    fn app_manager_set_neighbor_remote_max_debt(&mut self, 
                                                set_neighbor_remote_max_debt: SetNeighborRemoteMaxDebt) 
        -> Vec<MessengerTask> {

        unreachable!();
    }

    fn app_manager_reset_neighbor_channel(&mut self, 
                                          reset_neighbor_channel: ResetNeighborChannel) 
        -> Vec<MessengerTask> {

        unreachable!();
    }

    fn app_manager_set_neighbor_max_channels(&mut self, 
                                          set_neighbor_max_channels: SetNeighborMaxChannels) 
        -> Vec<MessengerTask> {

        unreachable!();
    }

    fn app_manager_add_neighbor(&mut self, add_neighbor: AddNeighbor) -> Vec<MessengerTask> {
        unreachable!();
    }

    fn app_manager_remove_neighbor(&mut self, remove_neighbor: RemoveNeighbor) -> Vec<MessengerTask> {
        unreachable!();
    }

    fn app_manager_set_neighbor_status(&mut self, set_neighbor_status: SetNeighborStatus) -> Vec<MessengerTask> {
        unreachable!();
    }

    pub fn handle_app_manager_message(&mut self, 
                                      networker_config: NetworkerConfig) -> Vec<MessengerTask> {
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
