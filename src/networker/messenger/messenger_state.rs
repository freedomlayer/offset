use std::collections::HashMap;
use std::net::SocketAddr;

use crypto::identity::PublicKey;

use proto::networker::ChannelToken;

use super::neighbor_tc_logic::NeighborTCState;
use super::types::NeighborTcOp;
use super::super::messages::{NeighborStatus};

use app_manager::messages::{SetNeighborMaxChannels, AddNeighbor, ResetNeighborChannel};

#[allow(dead_code)]
enum TokenChannelStatus {
    Valid,
    Inconsistent {
        current_token: ChannelToken,
        balance_for_reset: i64,
    },
}

#[allow(unused)]
pub struct TokenChannelSlot {
    tc_state: NeighborTCState,
    tc_status: TokenChannelStatus,
    pub pending_operations: Vec<NeighborTcOp>,
    // Pending operations to be sent to the token channel.
}

#[allow(unused)]
pub struct NeighborState {
    neighbor_socket_addr: Option<SocketAddr>, 
    remote_max_debt: u64,
    max_channels: u32,
    status: NeighborStatus,
    // Enabled or disabled?
    pub token_channel_slots: HashMap<u32, TokenChannelSlot>,
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
    pub neighbors: HashMap<PublicKey, NeighborState>,
}

pub enum AppManagerMessage {

}

pub enum FunderMessage {

}

pub enum ChannelerMessage {

}

pub enum CrypterMessage {

}

pub enum DatabaseMessage {
    SetNeighborMaxChannels(SetNeighborMaxChannels),
    ResetNeighborChannel(ResetNeighborChannel),
    AddNeighbor(AddNeighbor),
}


#[allow(unused)]
pub enum MessengerTask {
    AppManagerMessage(AppManagerMessage),
    FunderMessage(FunderMessage),
    ChannelerMessage(ChannelerMessage),
    CrypterMessage(CrypterMessage),
    DatabaseMessage(DatabaseMessage),
}

#[allow(unused)]
impl MessengerState {
    pub fn new() -> MessengerState {
        // TODO: Initialize from database somehow.
        unreachable!();
    }

    pub fn handle_timer_tick(&mut self) -> Vec<MessengerTask> {
        // TODO
        unreachable!();
    }

}

