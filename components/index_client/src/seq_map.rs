use std::collections::{HashMap, VecDeque};

pub struct SeqMap<K,V> {
    map: HashMap<K,V>,
    queue: VecDeque<K>,
    cycle_countdown: usize,
}

impl<K,V> SeqMap<K,V> 
where
    K: std::hash::Hash + std::cmp::Eq + Clone,
    V: Clone,
{
    pub fn new(map: HashMap<K,V>) -> Self {
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

    /// Return information of some current friend.
    ///
    /// Should return all friends after about n calls, where n is the amount of friends.
    /// This is important as the index server relies on this behaviour. If some friend is not
    /// returned after a large amount of calls, it will be deleted from the server.
    pub fn next(&mut self) -> Option<(usize, (K,V))> {
        self.queue.pop_front().map(|key| {
            // Move to the end of the queue:
            self.queue.push_back(key.clone());

            let value = self.map.get(&key).unwrap().clone();
            self.cycle_countdown = self.cycle_countdown.saturating_sub(1);

            (self.cycle_countdown, (key.clone(), value))
        })
    }
}

