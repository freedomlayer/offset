use std::collections::HashMap;
use std::net::SocketAddr;

use app_manager::messages::NetworkerConfig;
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
    fn new() -> MessengerState {
        // TODO: Initialize from database somehow.
        unreachable!();
    }
    fn config(&mut self, networker_config: &NetworkerConfig) -> Vec<MessengerTask> {
        // TODO
        
        match networker_config {
            NetworkerConfig::SetNeighborRemoteMaxDebt {
                neighbor_public_key, 
                remote_max_debt
            } => {},
            NetworkerConfig::ResetNeighborChannel {
                neighbor_public_key,
                channel_index,
                current_token,
                balance_for_reset,
            } => {},
            NetworkerConfig::SetNeighborMaxChannels {
                neighbor_public_key,
                max_channels
            } => {},
            NetworkerConfig::AddNeighbor {
                neighbor_public_key,
                neighbor_socket_addr,
                max_channels,
                remote_max_debt,
            } => {},
            NetworkerConfig::RemoveNeighbor {
                neighbor_public_key
            } => {},
            NetworkerConfig::SetNeighborStatus {
                neighbor_public_key, status
            } => {},
        }
        unreachable!();
    }

    fn channeler_message(&mut self) -> Vec<MessengerTask> {
        // TODO
        unreachable!();
    }

    fn funder_message(&mut self) -> Vec<MessengerTask> {
        // TODO
        unreachable!();
    }

    fn crypter_message(&mut self) -> Vec<MessengerTask> {
        // TODO
        unreachable!();
    }

    fn timer_tick(&mut self) -> Vec<MessengerTask> {
        // TODO
        unreachable!();
    }

}

