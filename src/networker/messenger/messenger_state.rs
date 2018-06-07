use std::collections::HashMap;
use std::net::SocketAddr;

use app_manager::messages::{NetworkerConfig, AddNeighbor, 
    RemoveNeighbor, SetNeighborStatus,  SetNeighborRemoteMaxDebt,
    ResetNeighborChannel, SetNeighborMaxChannels};
use crypto::identity::PublicKey;

use proto::networker::ChannelToken;

use super::neighbor_tc_logic::NeighborTCState;
use super::types::NeighborTcOp;
use super::super::messages::{NeighborStatus};

#[allow(dead_code)]
enum TokenChannelStatus {
    Valid,
    Inconsistent {
        current_token: ChannelToken,
        balance_for_reset: i64,
    },
}

#[allow(unused)]
struct TokenChannelSlot {
    tc_state: NeighborTCState,
    tc_status: TokenChannelStatus,
    pending_operations: Vec<NeighborTcOp>,
    // Pending operations to be sent to the token channel.
}

#[allow(unused)]
struct NeighborState {
    neighbor_socket_addr: Option<SocketAddr>, 
    remote_max_debt: u64,
    max_channels: u32,
    status: NeighborStatus,
    // Enabled or disabled?
    token_channels: HashMap<u32, TokenChannelSlot>,
    neighbor_pending_operations: Vec<NeighborTcOp>,
    // Pending operations that could be sent through any token channel.
    ticks_since_last_incoming: usize,
    // Number of time ticks since last incoming message
    ticks_since_last_outgoing: usize,
    // Number of time ticks since last outgoing message
    
    // TODO: Keep state of payment requests to Funder
    
    // TODO: Keep state of requests to database? Only write to RAM after getting acknowledgement
    // from database.
}

#[allow(unused)]
struct MessengerState {
    neighbors: HashMap<PublicKey, NeighborState>,
}

#[allow(unused)]
enum MessengerTask {
    SendAppManagerMessage,
    SendFunderMessage,
    SendChannelerMessage,
    SendCrypterMessage,
}

#[allow(unused)]
impl MessengerState {
    pub fn new() -> MessengerState {
        // TODO: Initialize from database somehow.
        unreachable!();
    }

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

    pub fn handle_channeler_message(&mut self) -> Vec<MessengerTask> {
        // TODO
        unreachable!();
    }

    pub fn handle_funder_message(&mut self) -> Vec<MessengerTask> {
        // TODO
        unreachable!();
    }

    pub fn handle_crypter_message(&mut self) -> Vec<MessengerTask> {
        // TODO
        unreachable!();
    }

    pub fn handle_timer_tick(&mut self) -> Vec<MessengerTask> {
        // TODO
        unreachable!();
    }

}

