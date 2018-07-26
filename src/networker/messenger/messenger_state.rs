use std::net::SocketAddr;
use im::hashmap::HashMap as ImHashMap;

use num_bigint::BigUint;
use num_traits::identities::Zero;

use crypto::identity::PublicKey;
// use crypto::rand_values::RandValue;

use super::neighbor::{NeighborState, NeighborMutation};


#[allow(unused)]
#[derive(Clone)]
pub struct MessengerState {
    local_public_key: PublicKey,
    neighbors: ImHashMap<PublicKey, NeighborState>,
}

#[allow(unused)]
pub enum MessengerMutation {
    NeighborMutation((PublicKey, NeighborMutation)),
    AddNeighbor((PublicKey, Option<SocketAddr>, u16)),
    // (neighbor_public_key, neighbor_addr, max_channels)
    RemoveNeighbor(PublicKey),
}


#[allow(unused)]
impl MessengerState {
    pub fn new() -> MessengerState {
        // TODO: Initialize from database somehow.
        unreachable!();
    }

    /// Get total trust (in credits) we put on all the neighbors together.
    pub fn get_total_trust(&self) -> BigUint {
        let mut sum: BigUint = BigUint::zero();
        for neighbor in self.neighbors.values() {
            sum += neighbor.get_trust();
        }
        sum
    }

    pub fn get_neighbors(&self) -> &ImHashMap<PublicKey, NeighborState> {
        &self.neighbors
    }

    pub fn get_local_public_key(&self) -> &PublicKey {
        &self.local_public_key
    }

    pub fn mutate(&mut self, messenger_mutation: &MessengerMutation) {
        match messenger_mutation {
            MessengerMutation::NeighborMutation((public_key, neighbor_mutation)) => {
                let neighbor = self.neighbors.get_mut(&public_key).unwrap();
                neighbor.mutate(neighbor_mutation);
            },
            MessengerMutation::AddNeighbor((neighbor_public_key, opt_socket_addr, max_channels)) => {
                let neighbor = NeighborState::new(*opt_socket_addr, *max_channels);
                // Insert neighbor, but also make sure that we did not remove any existing neighbor
                // with the same public key:
                let _ = self.neighbors.insert(neighbor_public_key.clone(), neighbor).unwrap();

            },
            MessengerMutation::RemoveNeighbor(public_key) => {
                let _ = self.neighbors.remove(&public_key);
            },
        }

    }
}
