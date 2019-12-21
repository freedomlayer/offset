use std::collections::{HashMap, VecDeque};

/// A basic map that allows to iterate over all its pairs in a consistent manner.
/// This means that calling SeqMap::next() repeatedly should iterate over all elements of the map,
/// even if elements are inserted or removed between SeqMap::next() calls.
pub struct SeqMap<K, V> {
    map: HashMap<K, V>,
    queue: VecDeque<K>,
    cycle_countdown: usize,
}

impl<K, V> SeqMap<K, V>
where
    K: std::hash::Hash + std::cmp::Eq + Clone,
    V: Clone,
{
    #[allow(unused)]
    pub fn new(map: HashMap<K, V>) -> Self {
        let queue = map
            .iter()
            .map(|(key, _)| key.clone())
            .collect::<VecDeque<_>>();

        let cycle_countdown = queue.len();

        SeqMap {
            map,
            queue,
            cycle_countdown,
        }
    }

    pub fn update(&mut self, key: K, value: V) -> Option<V> {
        // Put the newly updated item in the end of the queue:
        // TODO: Possibly optimize this later. Might be slow:
        self.queue.retain(|cur_key| cur_key != &key);
        self.queue.push_back(key.clone());
        self.map.insert(key, value)
    }

    pub fn remove(&mut self, key: &K) -> Option<V> {
        // Remove from queue:
        // TODO: Possibly optimize this later. Might be slow:
        self.queue.retain(|cur_key| cur_key != key);
        self.map.remove(key)
    }

    pub fn reset_countdown(&mut self) {
        self.cycle_countdown = self.queue.len();
    }

    /// Returns a pair (key, value) from the map.
    ///
    /// Guaranteed to return all pairs after about n calls, where n is the amount of pairs.
    /// It is guaranteed that after a reset_countdown() call, the following n next() calls should
    /// cover all the pairs that existed at the time reset_countdown() was called, even if update()
    /// and remove() were invoked.
    ///
    /// The user can know that all pairs were covered by checking if the returned cycle_countdown
    /// equals 0.
    pub fn next(&mut self) -> Option<(usize, (K, V))> {
        self.queue.pop_front().map(|key| {
            // Move to the end of the queue:
            self.queue.push_back(key.clone());

            let value = self.map.get(&key).unwrap().clone();
            self.cycle_countdown = self.cycle_countdown.saturating_sub(1);

            (self.cycle_countdown, (key, value))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Util test function to view the interval state of SeqMap
    fn seq_map_pairs<K, V>(seq_map: &mut SeqMap<K, V>) -> Vec<(K, V)>
    where
        K: std::hash::Hash + std::cmp::Eq + Clone + std::cmp::Ord,
        V: Clone + std::cmp::Ord,
    {
        seq_map.reset_countdown();

        let mut pairs = Vec::new();
        loop {
            let (countdown, pair) = seq_map.next().unwrap();
            pairs.push(pair);
            if countdown == 0 {
                break;
            }
        }
        pairs.sort();
        pairs
    }

    #[test]
    fn test_seq_map_update_remove() {
        let mut hash_map = HashMap::new();
        hash_map.insert(0u32, 4u64);
        hash_map.insert(1u32, 5u64);
        hash_map.insert(2u32, 6u64);

        let mut seq_map = SeqMap::new(hash_map);

        assert_eq!(seq_map_pairs(&mut seq_map), vec![(0, 4), (1, 5), (2, 6)]);

        seq_map.update(0u32, 5u64);
        seq_map.update(1u32, 6u64);
        seq_map.update(2u32, 7u64);

        assert_eq!(seq_map_pairs(&mut seq_map), vec![(0, 5), (1, 6), (2, 7)]);

        seq_map.remove(&1u32);

        assert_eq!(seq_map_pairs(&mut seq_map), vec![(0, 5), (2, 7)]);

        seq_map.update(3u32, 8u64);

        assert_eq!(seq_map_pairs(&mut seq_map), vec![(0, 5), (2, 7), (3, 8)]);
    }

    #[test]
    fn test_seq_map_next() {
        let mut hash_map = HashMap::new();
        hash_map.insert(0u32, 4u64);
        hash_map.insert(1u32, 5u64);
        hash_map.insert(2u32, 6u64);

        let mut seq_map = SeqMap::new(hash_map);

        let mut nexts = Vec::new();
        loop {
            let (countdown, (key, _value)) = seq_map.next().unwrap();
            nexts.push(key);
            if countdown == 0 {
                break;
            }
        }
        nexts.sort();
        assert_eq!(nexts, vec![0, 1, 2]);
    }

    #[test]
    fn test_seq_map_zero_countdown() {
        let mut hash_map = HashMap::new();
        hash_map.insert(0u32, 4u64);
        hash_map.insert(1u32, 5u64);
        hash_map.insert(2u32, 6u64);

        let mut seq_map = SeqMap::new(hash_map);

        let (countdown, _pair) = seq_map.next().unwrap();
        assert_eq!(countdown, 2);

        let (countdown, _pair) = seq_map.next().unwrap();
        assert_eq!(countdown, 1);

        let (countdown, _pair) = seq_map.next().unwrap();
        assert_eq!(countdown, 0);

        // We should keep getting 0 countdowns
        // after the first zero was encountered:
        for _ in 0..16 {
            let (countdown, _pair) = seq_map.next().unwrap();
            assert_eq!(countdown, 0);
        }

        seq_map.reset_countdown();

        let (countdown, _pair) = seq_map.next().unwrap();
        assert_eq!(countdown, 2);

        let (countdown, _pair) = seq_map.next().unwrap();
        assert_eq!(countdown, 1);

        let (countdown, _pair) = seq_map.next().unwrap();
        assert_eq!(countdown, 0);

        // We should keep getting 0 countdowns
        // after the first zero was encountered:
        for _ in 0..16 {
            let (countdown, _pair) = seq_map.next().unwrap();
            assert_eq!(countdown, 0);
        }
    }
}
