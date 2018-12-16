use crypto::identity::PublicKey;
use std::collections::HashMap;
use crypto::uid::Uid;
use crypto::hash::HashResult;
use proto::index::messages::{ForwardMutationsUpdate, TimeProofLink};
use crate::hash_clock::HashClock;

struct Verifier {
    hash_clock: HashClock<PublicKey>,
    node_states: HashMap<PublicKey, NodeState>, 
    node_time_to_live: usize,
}

struct NodeState {
    /// A randomly generated sessionId. The counter is related to this session Id.
    pub session_id: Uid,
    /// Incrementing counter, making sure that mutations are received in the correct order.
    /// For a new session, the counter should begin from 0 and increment by 1 for every MutationsUpdate message.
    /// When a new connection is established, a new sesionId should be randomly generated.
    pub counter: u64,
    /// This number decreases as time progresses.
    /// If it reaches 0, NodeState is removed.
    pub time_to_live: usize,
}

impl NodeState {
    fn new(session_id: Uid,
           counter: u64,
           time_to_live: usize) -> Self {

        NodeState {
            session_id,
            counter,
            time_to_live,
        }
    }
}


impl Verifier {
    pub fn new(last_ticks_max_len: usize,
               node_time_to_live: usize) -> Self {
        Verifier {
            hash_clock: HashClock::new(last_ticks_max_len),
            node_states: HashMap::new(),
            node_time_to_live,
        }
    }

    pub fn verify_forward_mutations_update(&mut self, 
                                       forward_mutations_update: &ForwardMutationsUpdate) 
                                        -> Option<TimeProofLink> {

        let ForwardMutationsUpdate {
            ref mutations_update,
            ref time_proof_chain,
        } = forward_mutations_update;

        // Verify signature:
        if !mutations_update.verify_signature() {
            return None;
        }

        // Check the hash time stamp:
        let expansion_chain = time_proof_chain
            .iter()
            .map(|time_proof_link| time_proof_link.hashes.clone())
            .collect::<Vec<_>>();

        let tick_hash = self.hash_clock.verify_expansion_chain(&mutations_update.time_hash,
                                               &expansion_chain)?;

        // Check counters (Maybe message is from the past?)
        match self.node_states.get_mut(&mutations_update.node_public_key) {
            None => {
                let node_state = NodeState::new(mutations_update.session_id, 
                                                mutations_update.counter,
                                                self.node_time_to_live);

                self.node_states.insert(mutations_update.node_public_key.clone(), node_state);
            },
            Some(node_state) => {
                if node_state.session_id != mutations_update.session_id {
                    node_state.session_id = mutations_update.session_id;
                    node_state.counter = mutations_update.counter;
                } else {
                    if node_state.counter >= mutations_update.counter {
                        return None;
                    }
                    node_state.counter = mutations_update.counter;
                }
            },
        };

        // If we got here, the message was new:
        let hashes = self.hash_clock.get_expansion(&tick_hash).unwrap().clone();
        Some(TimeProofLink {
            hashes,
        })
    }

    pub fn tick() -> HashResult {
        unimplemented!();
    }

    pub fn neighbor_tick() {
        unimplemented!();
    }
}

