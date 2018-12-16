use crypto::hash::HashResult;
use crypto::crypto_rand::RandValue;

use super::hash_clock::HashClock;
use super::ratchet::RatchetPool;
use super::verifier::Verifier;


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

}

impl<N,U> Verifier for SimpleVerifier<N,U>
where
    N: std::cmp::Eq + std::hash::Hash + Clone,
    U: std::cmp::Eq + Clone,
{
    type Node = N;
    type SessionId = U;

    fn verify(&mut self, 
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

    fn tick(&mut self, rand_value: RandValue) -> (HashResult, Vec<N>) {
        (self.hash_clock.tick(rand_value), self.ratchet_pool.tick())
    }

    fn neighbor_tick(&mut self, neighbor: N, tick_hash: HashResult) -> Option<HashResult> {
        self.hash_clock.neighbor_tick(neighbor, tick_hash)
    }

    fn remove_neighbor(&mut self, neighbor: &N) -> Option<HashResult> {
        self.hash_clock.remove_neighbor(neighbor)
    }
}

// TODO: Tests
