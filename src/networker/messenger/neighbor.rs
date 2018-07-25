use im::hashmap::HashMap as ImHashMap;
use im::vector::Vector;

use std::net::SocketAddr;

use num_bigint::BigUint;
use num_traits::identities::Zero;

use super::types::{RequestSendMessage};
use super::super::messages::{NeighborStatus};
use super::slot::TokenChannelSlot;


#[allow(unused)]
#[derive(Clone)]
pub struct NeighborState {
    neighbor_addr: Option<SocketAddr>, 
    pub local_max_channels: u16,
    pub remote_max_channels: u16,
    pub status: NeighborStatus,
    // Enabled or disabled?
    pub token_channel_slots: ImHashMap<u16, TokenChannelSlot>,
    pub pending_requests: Vector<RequestSendMessage>,
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
    pub fn new(neighbor_addr: Option<SocketAddr>,
               local_max_channels: u16) -> NeighborState {

        NeighborState {
            neighbor_addr,
            local_max_channels,
            remote_max_channels: local_max_channels,    
            // Initially we assume that the remote side has the same amount of channels as we do.
            status: NeighborStatus::Disable,
            token_channel_slots: ImHashMap::new(),
            pending_requests: Vector::new(),
            ticks_since_last_incoming: 0,
            ticks_since_last_outgoing: 0,
        }
    }

    /// Get the total trust we have in this neighbor.
    /// This is the total sum of all remote_max_debt for all the token channels we have with this
    /// neighbor. In other words, this is the total amount of money we can lose if this neighbor
    /// leaves and never returns.
    pub fn get_trust(&self) -> BigUint {
        let mut sum: BigUint = BigUint::zero();
        for token_channel_slot in self.token_channel_slots.values() {
            let remote_max_debt: BigUint = token_channel_slot.wanted_remote_max_debt.into();
            sum += remote_max_debt;
        }
        sum
    }
}
