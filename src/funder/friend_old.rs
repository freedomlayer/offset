use im::hashmap::HashMap as ImHashMap;
use im::vector::Vector;

use std::net::SocketAddr;
use std::cmp;

use crypto::identity::PublicKey;

use num_bigint::BigUint;
use num_traits::identities::Zero;

use super::types::{RequestSendMessage};
use super::messages::{FriendStatus};
use super::slot::{TokenChannelSlot, SlotMutation};


#[allow(unused)]
#[derive(Clone)]
pub struct FriendState {
    pub local_public_key: PublicKey,
    pub remote_public_key: PublicKey,
    friend_addr: Option<SocketAddr>, 
    pub local_max_channels: u16,
    pub remote_max_channels: u16,
    pub status: FriendStatus,
    // Enabled or disabled?
    pub tc_slot: TokenChannelSlot,
    pub pending_requests: Vector<RequestSendMessage>,
    // Pending operations that could be sent through any token channel.
    ticks_since_last_incoming: usize,
    // Number of time ticks since last incoming message
    ticks_since_last_outgoing: usize,
    // Number of time ticks since last outgoing message
}

#[allow(unused)]
pub enum FriendMutation {
    SetLocalMaxChannels(u16),
    SetRemoteMaxChannels(u16),
    SetStatus(FriendStatus),
    PushBackPendingRequest(RequestSendMessage),
    PopFrontPendingRequest,
    SlotMutation((u16, SlotMutation)),
}

#[allow(unused)]
impl FriendState {
    pub fn new(local_public_key: &PublicKey,
               remote_public_key: &PublicKey,
               friend_addr: Option<SocketAddr>,
               local_max_channels: u16) -> FriendState {

        FriendState {
            local_public_key: local_public_key.clone(),
            remote_public_key: remote_public_key.clone(),
            friend_addr,
            local_max_channels,
            remote_max_channels: local_max_channels,    
            // Initially we assume that the remote side has the same amount of channels as we do.
            status: FriendStatus::Disable,
            tc_slot: TokenChannelSlot::new(),
            pending_requests: Vector::new(),
            ticks_since_last_incoming: 0,
            ticks_since_last_outgoing: 0,
        }
    }

    /// Get the total trust we have in this friend.
    /// This is the total sum of all remote_max_debt for all the token channels we have with this
    /// friend. In other words, this is the total amount of money we can lose if this friend
    /// leaves and never returns.
    pub fn get_trust(&self) -> BigUint {
        let mut sum: BigUint = BigUint::zero();
        for token_channel_slot in self.tc_slots.values() {
            let remote_max_debt: BigUint = token_channel_slot.wanted_remote_max_debt.into();
            sum += remote_max_debt;
        }
        sum
    }

    pub fn mutate(&mut self, friend_mutation: &FriendMutation) {
        match friend_mutation {
            FriendMutation::SetLocalMaxChannels(local_max_channels) => {
                self.local_max_channels = *local_max_channels;
            },
            FriendMutation::SetRemoteMaxChannels(remote_max_channels) => {
                self.remote_max_channels = *remote_max_channels;
            },
            FriendMutation::SetStatus(friend_status) => {
                self.status = *friend_status;
            },
            FriendMutation::PushBackPendingRequest(request) => {
                self.pending_requests.push_back(request.clone());
            },
            FriendMutation::PopFrontPendingRequest => {
                let _ = self.pending_requests.pop_front();
            },
            FriendMutation::SlotMutation((slot_index, slot_mutation)) => {
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
