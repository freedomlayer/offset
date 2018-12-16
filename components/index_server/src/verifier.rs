use crypto::hash::HashResult;
use crypto::crypto_rand::RandValue;

use crate::hash_clock::HashClock;
use crate::ratchet::RatchetPool;


pub trait Verifier {
    type Node;
    type SessionId;

    fn verify(&mut self, 
                   origin_tick_hash: &HashResult,
                   expansion_chain: &[Vec<HashResult>],
                   node: &Self::Node,
                   session_id: &Self::SessionId,
                   counter: u64) -> Option<Vec<HashResult>>;

    fn tick(&mut self, rand_value: RandValue) -> HashResult;
    fn neighbor_tick(&mut self, neighbor: Self::Node, tick_hash: HashResult);
    fn remove_neighbor(&mut self, neighbor: &Self::Node);
}


struct SimpleVerifier<N,U> {
    hash_clock: HashClock<N>,
    ratchet_pool: RatchetPool<N,U>,
}

impl<N,U> SimpleVerifier<N,U> 
where
    N: std::cmp::Eq + std::hash::Hash + Clone,
    U: std::cmp::Eq + Clone,
{

    pub fn new(ticks_to_live: usize) -> Self {
        // TODO(Security): Make sure that we don't have an off-by-one here with the decision to have
        // one ticks_to_live value for both `hash_clock` and `ratchet_pool`.
       
        assert!(ticks_to_live > 0);
        
        SimpleVerifier {
            hash_clock: HashClock::new(ticks_to_live),
            ratchet_pool: RatchetPool::new(ticks_to_live),
        }
    }

    pub fn verify(&mut self, 
                   origin_tick_hash: &HashResult,
                   expansion_chain: &[Vec<HashResult>],
                   node: &N,
                   session_id: &U,
                   counter: u64) -> Option<Vec<HashResult>> {

        // Check the hash time stamp:
        let tick_hash = self.hash_clock.verify_expansion_chain(origin_tick_hash,
                                               expansion_chain)?;

        // Update ratchets (This should protect against out of order messages):
        if !self.ratchet_pool.update(node, session_id, counter) {
            return None;
        }

        // If we got here, the message was new:
        let hashes = self.hash_clock.get_expansion(&tick_hash).unwrap().clone();
        Some(hashes)
    }

    pub fn tick(&mut self, rand_value: RandValue) -> HashResult {
        self.ratchet_pool.tick();
        self.hash_clock.tick(rand_value)
    }

    pub fn neighbor_tick(&mut self, neighbor: N, tick_hash: HashResult) -> Option<HashResult> {
        self.hash_clock.neighbor_tick(neighbor, tick_hash)
    }
}

