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


#[allow(unused)]
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

#[allow(unused)]
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
    local_public_key: PublicKey,
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


#[allow(unused)]
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

    pub fn get_local_public_key(&self) -> &PublicKey {
        &self.local_public_key
    }

    pub fn set_neighbor_remote_max_debt(&mut self, 
                                        set_neighbor_remote_max_debt: SetNeighborRemoteMaxDebt)
                                        -> Result<Vec<DatabaseMessage>, MessengerStateError> {

        let neighbor_state = self.neighbors.get_mut(&set_neighbor_remote_max_debt.neighbor_public_key)
            .ok_or(MessengerStateError::NeighborDoesNotExist)?;
        
        // Find the token channel slot:
        let token_channel_slot = neighbor_state.token_channel_slots
            .get_mut(&set_neighbor_remote_max_debt.channel_index)
            .ok_or(MessengerStateError::TokenChannelDoesNotExist)?;

        token_channel_slot.wanted_remote_max_debt = set_neighbor_remote_max_debt.remote_max_debt;
        Ok(vec![DatabaseMessage::SetNeighborRemoteMaxDebt(set_neighbor_remote_max_debt)])
    }


    pub fn reset_neighbor_channel(&mut self, 
                                    reset_neighbor_channel: ResetNeighborChannel) 
                                    -> Result<Vec<DatabaseMessage>, MessengerStateError> {
                                        
        let neighbor_state = self.neighbors.get_mut(&reset_neighbor_channel.neighbor_public_key)
            .ok_or(MessengerStateError::NeighborDoesNotExist)?;

        let new_token_channel_slot = TokenChannelSlot::new_from_reset(
            &self.local_public_key,
            &reset_neighbor_channel.neighbor_public_key,
            &reset_neighbor_channel.current_token,
            reset_neighbor_channel.balance_for_reset);

        // Replace the old token channel slot with the new one:
        if !neighbor_state.token_channel_slots.contains_key(&reset_neighbor_channel.channel_index) {
            return Err(MessengerStateError::TokenChannelDoesNotExist);
        }
        
        neighbor_state.token_channel_slots.insert(reset_neighbor_channel.channel_index, 
                                                  new_token_channel_slot);

        Ok(vec![DatabaseMessage::ResetNeighborChannel(reset_neighbor_channel)])
    }

    pub fn set_neighbor_max_channels(&mut self, 
                                    set_neighbor_max_channels: SetNeighborMaxChannels) 
                                    -> Result<Vec<DatabaseMessage>, MessengerStateError> {

        // Check if we have the requested neighbor:
        let neighbor_state = self.neighbors.get_mut(&set_neighbor_max_channels.neighbor_public_key)
            .ok_or(MessengerStateError::NeighborDoesNotExist)?;

        neighbor_state.local_max_channels = set_neighbor_max_channels.max_channels;

        Ok(vec![DatabaseMessage::SetNeighborMaxChannels(set_neighbor_max_channels)])
    }

    pub fn add_neighbor(&mut self, 
                        add_neighbor: AddNeighbor) 
                        -> Result<Vec<DatabaseMessage>, MessengerStateError> {

        // If we already have the neighbor: return error.
        if self.neighbors.contains_key(&add_neighbor.neighbor_public_key) {
            return Err(MessengerStateError::NeighborAlreadyExists);
        }

        // Otherwise, we add a new neighbor:
        let neighbor_state = NeighborState::new(
                add_neighbor.neighbor_socket_addr,
                add_neighbor.max_channels);

        self.neighbors.insert(add_neighbor.neighbor_public_key.clone(), neighbor_state);

        Ok(vec![DatabaseMessage::AddNeighbor(add_neighbor)])
    }
}
