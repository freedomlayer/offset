use std::collections::HashMap;
use std::net::SocketAddr;

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
pub struct MessengerState {
    neighbors: HashMap<PublicKey, NeighborState>,
}

#[allow(unused)]
pub enum MessengerTask {
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

