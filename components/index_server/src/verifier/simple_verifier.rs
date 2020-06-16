use crypto::rand::{CryptoRandom, RandGen};

use proto::crypto::{HashResult, RandValue};

use super::hash_clock::HashClock;
use super::ratchet::RatchetPool;
use super::verifier::Verifier;

pub struct SimpleVerifier<N, B, U, R> {
    hash_clock: HashClock<B>,
    ratchet_pool: RatchetPool<N, U>,
    rng: R,
}

impl<N, B, U, R> SimpleVerifier<N, B, U, R>
where
    N: std::cmp::Eq + std::hash::Hash + Clone,
    B: std::cmp::Eq + std::hash::Hash + Clone,
    U: std::cmp::Eq + Clone,
    R: CryptoRandom,
{
    #[allow(unused)]
    pub fn new(ticks_to_live: usize, rng: R) -> Self {
        // TODO: Security: Make sure that we don't have an off-by-one here with the decision to have
        // one ticks_to_live value for both `hash_clock` and `ratchet_pool`.

        assert!(ticks_to_live > 0);

        SimpleVerifier {
            hash_clock: HashClock::new(ticks_to_live),
            ratchet_pool: RatchetPool::new(ticks_to_live),
            rng,
        }
    }
}

impl<N, B, U, R> Verifier for SimpleVerifier<N, B, U, R>
where
    N: std::cmp::Eq + std::hash::Hash + Clone,
    B: std::cmp::Eq + std::hash::Hash + Clone,
    U: std::cmp::Eq + Clone,
    R: CryptoRandom,
{
    type Node = N;
    type Neighbor = B;
    type SessionId = U;

    fn verify(
        &mut self,
        origin_tick_hash: &HashResult,
        expansion_chain: &[&[HashResult]],
        node: &N,
        session_id: &U,
        counter: u64,
    ) -> Option<&[HashResult]> {
        // Check the hash time stamp:
        let tick_hash = self
            .hash_clock
            .verify_expansion_chain(origin_tick_hash, expansion_chain)?;

        // Update ratchets (This should protect against out of order messages):
        if !self.ratchet_pool.update(node, session_id, counter) {
            return None;
        }

        // If we got here, the message was new:
        let hashes = self.hash_clock.get_expansion(&tick_hash).unwrap();
        Some(hashes)
    }

    fn tick(&mut self) -> (HashResult, Vec<N>) {
        let rand_value = RandValue::rand_gen(&mut self.rng);
        (self.hash_clock.tick(rand_value), self.ratchet_pool.tick())
    }

    fn neighbor_tick(&mut self, neighbor: B, tick_hash: HashResult) -> Option<HashResult> {
        self.hash_clock.neighbor_tick(neighbor, tick_hash)
    }

    fn remove_neighbor(&mut self, neighbor: &B) -> Option<HashResult> {
        self.hash_clock.remove_neighbor(neighbor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crypto::test_utils::DummyRandom;

    #[test]
    fn test_simple_verifier_basic() {
        let ticks_to_live = 8;
        let num_verifiers = 4;

        let mut svs = Vec::new();

        for i in 0..num_verifiers {
            let rng = DummyRandom::new(&[i as u8]);
            svs.push(SimpleVerifier::new(ticks_to_live, rng));
        }

        for _iter in 0..ticks_to_live + 1 {
            for i in 0..num_verifiers {
                let (tick_hash, _removed) = svs[i].tick();
                for j in 0..num_verifiers {
                    if j == i {
                        continue;
                    }
                    let _ = svs[j].neighbor_tick(i, tick_hash.clone());
                }
            }
        }

        let (tick_hash, _removed) = svs[0].tick();

        // Forwarding of a message:
        let hashes0 = svs[0]
            .verify(&tick_hash, &[], &1234u128, &0u128, 0u64)
            .unwrap()
            .to_vec();
        let hashes1 = svs[1]
            .verify(&tick_hash, &[&hashes0], &1234u128, &0u128, 0u64)
            .unwrap()
            .to_vec();
        let hashes2 = svs[2]
            .verify(&tick_hash, &[&hashes0, &hashes1], &1234u128, &0u128, 0u64)
            .unwrap()
            .to_vec();
        let _hashes3 = svs[3]
            .verify(
                &tick_hash,
                &[&hashes0, &hashes1, &hashes2],
                &1234u128,
                &0u128,
                0u64,
            )
            .unwrap()
            .to_vec();
    }

    // TODO: Add more tests?
}
