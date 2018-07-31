use im::hashmap::HashMap as ImHashMap;
use im::vector::Vector;

use std::net::SocketAddr;
use std::cmp;

use crypto::identity::PublicKey;

use num_bigint::BigUint;
use num_traits::identities::Zero;

use super::types::{RequestSendMessage};
use super::super::messages::{NeighborStatus};
use super::slot::{TokenChannelSlot, SlotMutation};


#[allow(unused)]
#[derive(Clone)]
pub struct NeighborState {
    pub local_public_key: PublicKey,
    pub remote_public_key: PublicKey,
    neighbor_addr: Option<SocketAddr>, 
    pub local_max_channels: u16,
    pub remote_max_channels: u16,
    pub status: NeighborStatus,
    // Enabled or disabled?
    pub tc_slots: ImHashMap<u16, TokenChannelSlot>,
    pub pending_requests: Vector<RequestSendMessage>,
    // Pending operations that could be sent through any token channel.
    ticks_since_last_incoming: usize,
    // Number of time ticks since last incoming message
    ticks_since_last_outgoing: usize,
    // Number of time ticks since last outgoing message
}

#[allow(unused)]
pub enum NeighborMutation {
    SetLocalMaxChannels(u16),
    SetRemoteMaxChannels(u16),
    SetStatus(NeighborStatus),
    PushBackPendingRequest(RequestSendMessage),
    PopFrontPendingRequest,
    SlotMutation((u16, SlotMutation)),
}

#[allow(unused)]
impl NeighborState {
    pub fn new(local_public_key: &PublicKey,
               remote_public_key: &PublicKey,
               neighbor_addr: Option<SocketAddr>,
               local_max_channels: u16) -> NeighborState {

        NeighborState {
            local_public_key: local_public_key.clone(),
            remote_public_key: remote_public_key.clone(),
            neighbor_addr,
            local_max_channels,
            remote_max_channels: local_max_channels,    
            // Initially we assume that the remote side has the same amount of channels as we do.
            status: NeighborStatus::Disable,
            tc_slots: ImHashMap::new(),
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
        for token_channel_slot in self.tc_slots.values() {
            let remote_max_debt: BigUint = token_channel_slot.wanted_remote_max_debt.into();
            sum += remote_max_debt;
        }
        sum
    }

    pub fn mutate(&mut self, neighbor_mutation: &NeighborMutation) {
        match neighbor_mutation {
            NeighborMutation::SetLocalMaxChannels(local_max_channels) => {
                self.local_max_channels = *local_max_channels;
            },
            NeighborMutation::SetRemoteMaxChannels(remote_max_channels) => {
                self.remote_max_channels = *remote_max_channels;
            },
            NeighborMutation::SetStatus(neighbor_status) => {
                self.status = neighbor_status.clone();
            },
            NeighborMutation::PushBackPendingRequest(request) => {
                self.pending_requests.push_back(request.clone());
            },
            NeighborMutation::PopFrontPendingRequest => {
                let _ = self.pending_requests.pop_front();
            },
            NeighborMutation::SlotMutation((slot_index, slot_mutation)) => {
                // If the slot does not exist but it is in reasonable index boundaries, we create
                // it. If it is outside the reasonable boundaries and it does not exist, we panic.
                let tc_slot = if *slot_index < cmp::min(self.local_max_channels, self.remote_max_channels) {
                    self.tc_slots.entry(*slot_index).or_insert(
                        TokenChannelSlot::new(
                            &self.local_public_key, &self.remote_public_key, *slot_index))
                } else {
                    self.tc_slots.get_mut(slot_index).unwrap()
                };
                tc_slot.mutate(slot_mutation);
            },
        }
    }
}
