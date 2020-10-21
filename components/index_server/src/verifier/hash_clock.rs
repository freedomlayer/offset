use crypto::hash::hash_buffer;
use proto::crypto::{HashResult, RandValue};

use std::collections::{HashMap, VecDeque};

/// Prefix put before hashing a list of hashes:
const HASH_CLOCK_PREFIX: &[u8] = b"HASH_CLOCK";

pub struct HashClock<N> {
    /// Last hash we received from each neighbor
    neighbor_hashes: HashMap<N, HashResult>,
    /// Maximum length of last_ticks:
    last_ticks_max_len: usize,
    last_ticks: VecDeque<HashResult>,
    last_ticks_map: HashMap<HashResult, Vec<HashResult>>,
}

/// Combine a list of hashes into one hash result:
fn hash_hashes(hashes: &[HashResult]) -> HashResult {
    let mut bytes_to_hash = Vec::new();

    // Start with a constant prefix:
    bytes_to_hash.extend_from_slice(&hash_buffer(HASH_CLOCK_PREFIX));

    // Append prefixes:
    for hash in hashes {
        bytes_to_hash.extend_from_slice(hash);
    }

    hash_buffer(&bytes_to_hash)
}

impl<N> HashClock<N>
where
    N: std::hash::Hash + std::cmp::Eq + Clone,
{
    pub fn new(last_ticks_max_len: usize) -> Self {
        assert!(last_ticks_max_len > 0);

        HashClock {
            neighbor_hashes: HashMap::new(),
            last_ticks_max_len,
            last_ticks: VecDeque::new(),
            last_ticks_map: HashMap::new(),
        }
    }

    /// Insert a new pair of (hash, hash_info)
    fn insert_tick_hash(&mut self, tick_hash: HashResult, expansion: Vec<HashResult>) {
        assert!(self.last_ticks_max_len > 0);
        self.last_ticks.push_back(tick_hash.clone());

        if self.last_ticks.len() > self.last_ticks_max_len {
            let popped_tick_hash = self.last_ticks.pop_front().unwrap();
            self.last_ticks_map.remove(&popped_tick_hash);
        }

        assert!(self.last_ticks.len() <= self.last_ticks_max_len);
        self.last_ticks_map.insert(tick_hash, expansion);
    }

    /// Should be called when a new hash is received from a neighbor.
    pub fn neighbor_tick(&mut self, neighbor: N, tick_hash: HashResult) -> Option<HashResult> {
        self.neighbor_hashes.insert(neighbor, tick_hash)
    }

    /// Remove a neighbor from the HashClock.
    pub fn remove_neighbor(&mut self, neighbor: &N) -> Option<HashResult> {
        self.neighbor_hashes.remove(neighbor)
    }

    /// Advance the time in the clock by one tick.
    /// The resulting tick_hash should be sent to all the neighbors.
    pub fn tick(&mut self, rand_value: RandValue) -> HashResult {
        let mut expansion = Vec::new();

        let hashed_rand_value = hash_buffer(&rand_value);
        expansion.push(hashed_rand_value);

        // Concatenate all hashes, and update hash_info accordingly:
        for hash in self.neighbor_hashes.values() {
            expansion.push(hash.clone());
        }

        let tick_hash = hash_hashes(&expansion);
        self.insert_tick_hash(tick_hash.clone(), expansion);

        tick_hash
    }

    /// Given a tick hash (that was created in this HashClock), create a hash proof for a neighbor.
    pub fn get_expansion(&self, tick_hash: &HashResult) -> Option<&[HashResult]> {
        self.last_ticks_map
            .get(tick_hash)
            .map(|expansion| &expansion[..])
    }

    /// Verify a chain of hash proof links.
    /// Each link shows that a certain hash is composed from a list of hashes.
    /// Eventually one of those hashes is a tick_hash created at this HashClock.
    /// This proves that the `origin_tick_hash` is recent.
    pub fn verify_expansion_chain(
        &self,
        origin_tick_hash: &HashResult,
        expansion_chain: &[&[HashResult]],
    ) -> Option<&HashResult> {
        /*                       +-/hash0    +-/hash0    +-/hash0
         *  `origin_tick_hash` --+  hash1    |  hash1    |  hash1 -- found in `last_ticks_map`
         *                       |  hash2 ---+  hash2    +-\hash2
         *                       +-\hash3    |  hash3 ---+
         *                                   +-\hash4
         */

        // Add origin_tick_hash as a list of 1 hashes in the beginning of hash_proof_chain.
        // This allows dealing with some edge cases more smoothly.
        let mut ex_expansion_chain: Vec<&[HashResult]> = Vec::new();
        let first_hashes = &[origin_tick_hash.clone()];
        ex_expansion_chain.push(&first_hashes[..]);

        for hashes in expansion_chain {
            ex_expansion_chain.push(hashes);
        }

        // Calculate hashes of all lists of hashes
        let hash_res = expansion_chain
            .iter()
            .map(|hashes| hash_hashes(hashes))
            .collect::<Vec<_>>();

        for i in 0..hash_res.len() {
            if !ex_expansion_chain[i].contains(&hash_res[i]) {
                return None;
            }
        }

        for hash in ex_expansion_chain.last().unwrap().iter() {
            if let Some((hash, _)) = self.last_ticks_map.get_key_value(hash) {
                return Some(hash);
            }
        }
        None
    }

    #[allow(unused)]
    pub fn get_neighbor_hash(&self, neighbor: &N) -> Option<&HashResult> {
        self.neighbor_hashes.get(neighbor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_clock_basic() {
        let last_ticks_max_len = 4;

        let mut hash_clocks = Vec::new();
        for _ in 0..4 {
            hash_clocks.push(HashClock::new(last_ticks_max_len));
        }

        // Some iterations of communication between the participants:
        for iter in 0..8 {
            for j in 0..hash_clocks.len() {
                let rand_value = RandValue::from(&[iter as u8; RandValue::len()]);
                let tick_hash = hash_clocks[j].tick(rand_value);
                for k in 0..hash_clocks.len() {
                    if k == j {
                        continue;
                    }
                    hash_clocks[k].neighbor_tick(j, tick_hash.clone());
                }
            }
        }

        let origin_tick_hash = hash_clocks[0].get_neighbor_hash(&1).unwrap().clone();
        let tick_hash1 = hash_clocks[1]
            .verify_expansion_chain(&origin_tick_hash, &[])
            .unwrap();

        let expansion1 = hash_clocks[1].get_expansion(&tick_hash1).unwrap().to_vec();
        let tick_hash2 = hash_clocks[2]
            .verify_expansion_chain(&origin_tick_hash, &[&expansion1])
            .unwrap();

        let expansion2 = hash_clocks[2].get_expansion(&tick_hash2).unwrap().to_vec();
        let tick_hash3 = hash_clocks[3]
            .verify_expansion_chain(&origin_tick_hash, &[&expansion1, &expansion2])
            .unwrap();

        let expansion3 = hash_clocks[3].get_expansion(&tick_hash3).unwrap().to_vec();
        let _tick_hash3 = hash_clocks[0]
            .verify_expansion_chain(&origin_tick_hash, &[&expansion1, &expansion2, &expansion3])
            .unwrap();

        // Everything is forgotten after `last_ticks_max_len` ticks:
        for iter in 0..last_ticks_max_len {
            let rand_value = RandValue::from(&[iter as u8; RandValue::len()]);
            hash_clocks[0].tick(rand_value);
        }

        assert!(hash_clocks[0]
            .verify_expansion_chain(&origin_tick_hash, &[&expansion1, &expansion2, &expansion3])
            .is_none());
    }
}
