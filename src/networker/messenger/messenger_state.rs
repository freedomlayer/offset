use std::collections::{HashMap, VecDeque};
use std::net::SocketAddr;

use crypto::identity::PublicKey;

use proto::networker::ChannelToken;

use super::neighbor_tc_logic::NeighborTCState;
use super::types::NeighborTcOp;
use super::super::messages::{NeighborStatus};

use app_manager::messages::{SetNeighborRemoteMaxDebt, SetNeighborMaxChannels, AddNeighbor, RemoveNeighbor, 
    ResetNeighborChannel, SetNeighborStatus};

// use super::handle_neighbor::{NeighborMoveToken, NeighborInconsistencyError, NeighborSetMaxTokenChannels};

#[allow(dead_code)]
pub enum TokenChannelStatus {
    Valid,
    Inconsistent {
        current_token: ChannelToken,
        balance_for_reset: i64,
    },
}

#[allow(unused)]
pub struct TokenChannelSlot {
    pub tc_state: NeighborTCState,
    pub tc_status: TokenChannelStatus,
    pub wanted_remote_max_debt: u64,
    pub pending_operations: VecDeque<NeighborTcOp>,
    // Pending operations to be sent to the token channel.
}


impl TokenChannelSlot {
    pub fn new(local_public_key: &PublicKey,
               remote_public_key: &PublicKey,
               token_channel_index: u16) -> TokenChannelSlot {
        TokenChannelSlot {
            tc_state: NeighborTCState::new(local_public_key,
                                           remote_public_key,
                                           token_channel_index),
            tc_status: TokenChannelStatus::Valid,
            wanted_remote_max_debt: 0,
            pending_operations: VecDeque::new(),
        }
    }

    pub fn new_from_reset(local_public_key: &PublicKey,
                           remote_public_key: &PublicKey,
                           current_token: &ChannelToken,
                           balance: i64) -> TokenChannelSlot {

        TokenChannelSlot {
            tc_state: NeighborTCState::new_from_reset(local_public_key,
                                                      remote_public_key,
                                                      current_token,
                                                      balance),
            tc_status: TokenChannelStatus::Valid,
            wanted_remote_max_debt: 0,
            pending_operations: VecDeque::new(),
        }
    }
}

#[allow(unused)]
pub struct NeighborState {
    neighbor_socket_addr: Option<SocketAddr>, 
    pub local_max_channels: u16,
    pub remote_max_channels: u16,
    pub status: NeighborStatus,
    // Enabled or disabled?
    pub token_channel_slots: HashMap<u16, TokenChannelSlot>,
    neighbor_pending_operations: VecDeque<NeighborTcOp>,
    // Pending operations that could be sent through any token channel.
    ticks_since_last_incoming: usize,
    // Number of time ticks since last incoming message
    ticks_since_last_outgoing: usize,
    // Number of time ticks since last outgoing message
    
    // TODO: Keep state of payment requests to Funder
    
    // TODO: Keep state of requests to database? Only write to RAM after getting acknowledgement
    // from database.
}

impl NeighborState {
    pub fn new(neighbor_socket_addr: Option<SocketAddr>,
               local_max_channels: u16) -> NeighborState {

        NeighborState {
            neighbor_socket_addr,
            local_max_channels,
            remote_max_channels: local_max_channels,    
            // Initially we assume that the remote side has the same amount of channels as we do.
            status: NeighborStatus::Disable,
            token_channel_slots: HashMap::new(),
            neighbor_pending_operations: VecDeque::new(),
            ticks_since_last_incoming: 0,
            ticks_since_last_outgoing: 0,
        }
    }
}

#[allow(unused)]
pub struct MessengerState {
    neighbors: HashMap<PublicKey, NeighborState>,
}

#[allow(unused)]
pub enum DatabaseMessage {
    SetNeighborRemoteMaxDebt(SetNeighborRemoteMaxDebt),
    SetNeighborMaxChannels(SetNeighborMaxChannels),
    ResetNeighborChannel(ResetNeighborChannel),
    AddNeighbor(AddNeighbor),
    RemoveNeighbor(RemoveNeighbor),
    SetNeighborStatus(SetNeighborStatus),
}


pub enum MessengerStateError {
    NeighborDoesNotExist,
    TokenChannelDoesNotExist,
    NeighborAlreadyExists,
}

#[allow(unused)]
impl MessengerState {
    pub fn new() -> MessengerState {
        // TODO: Initialize from database somehow.
        unreachable!();
    }

    pub fn get_neighbors(&self) -> &HashMap<PublicKey, NeighborState> {
        &self.neighbors
    }

    pub fn set_neighbor_remote_max_debt(&mut self, 
                                        db_messages: &mut Vec<DatabaseMessage>,
                                        neighbor_public_key: &PublicKey,
                                        channel_index: u16,
                                        remote_max_debt: u64) -> Result<(), MessengerStateError> {

        let neighbor_state = self.neighbors.get_mut(neighbor_public_key)
            .ok_or(MessengerStateError::NeighborDoesNotExist)?;
        
        // Find the token channel slot:
        let token_channel_slot = neighbor_state.token_channel_slots.get_mut(&channel_index)
            .ok_or(MessengerStateError::TokenChannelDoesNotExist)?;

        token_channel_slot.wanted_remote_max_debt = remote_max_debt;
        db_messages.push(DatabaseMessage::SetNeighborRemoteMaxDebt(SetNeighborRemoteMaxDebt {
            neighbor_public_key: neighbor_public_key.clone(),
            channel_index,
            remote_max_debt,
        }));

        Ok(())
    }
}
