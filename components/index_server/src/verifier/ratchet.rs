use std::collections::HashMap;

struct Ratchet<U> {
    /// A randomly generated sessionId. The counter is related to this session Id.
    pub session_id: U,
    /// Incrementing counter, making sure that mutations are received in the correct order.
    /// For a new session, the counter should begin from 0 and increment by 1 for every MutationsUpdate message.
    /// When a new connection is established, a new sesionId should be randomly generated.
    pub counter: u64,
    /// Initial value for cur_ticks_to_live
    pub ticks_to_live: usize,
    /// This number decreases as time progresses.
    /// If it reaches 0, NodeState is removed.
    pub cur_ticks_to_live: usize,
}

impl<U> Ratchet<U> 
where
    U: std::cmp::Eq + Clone,
{
    pub fn new(session_id: U,
           counter: u64,
           ticks_to_live: usize) -> Self {

        let cur_ticks_to_live = ticks_to_live;
        Ratchet {
            session_id,
            counter,
            ticks_to_live,
            cur_ticks_to_live,
        }
    }

    fn inner_update(&mut self, session_id: &U, counter: u64) -> bool {
        if &self.session_id != session_id {
            self.session_id = session_id.clone();
            self.counter = counter;
            return true;
        } 
        // self.session_id == session_id:
        if self.counter < counter {
            self.counter = counter;
            return true;
        }
        false
    }

    pub fn update(&mut self, session_id: &U, counter: u64) -> bool {
        if !self.inner_update(session_id, counter) {
            false
        } else {
            self.cur_ticks_to_live = self.ticks_to_live;
            true
        }
    }

    pub fn tick(&mut self) -> usize {
        self.cur_ticks_to_live = self.cur_ticks_to_live.saturating_sub(1);
        self.cur_ticks_to_live
    }
}

pub struct RatchetPool<N,U> {
    ratchets: HashMap<N, Ratchet<U>>,
    ratchet_ticks_to_live: usize,
}

impl<N,U> RatchetPool<N,U> 
where
    N: std::cmp::Eq + std::hash::Hash + Clone,
    U: std::cmp::Eq + Clone,
{
    pub fn new(ratchet_ticks_to_live: usize) -> Self {
        RatchetPool {
            ratchets: HashMap::new(),
            ratchet_ticks_to_live,
        }
    }

    pub fn tick(&mut self) -> Vec<N> {
        let mut removed_nodes = Vec::new();
        for (node, ratchet) in &mut self.ratchets {
            if ratchet.tick() == 0 {
                removed_nodes.push(node.clone());
            }
        }
        for removed_node in &removed_nodes {
            self.ratchets.remove(removed_node);
        }
        removed_nodes
    }

    /// Try to update a certain ratchet.
    /// Returns true if ratchet moved forward (Or created)
    pub fn update(&mut self, node: &N, session_id: &U, counter: u64) -> bool {
        let ratchet = match self.ratchets.get_mut(node) {
            None => {
                let ratchet = Ratchet::new(session_id.clone(), 
                                                counter,
                                                self.ratchet_ticks_to_live);
                self.ratchets.insert(node.clone(), ratchet);
                return true;
            },
            Some(ratchet) => ratchet,
        };
        ratchet.update(session_id, counter)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ratchet_update() {
        let mut session_id = 0u128;
        let mut counter = 0u64;
        let ticks_to_live = 8;

        let mut ratchet = Ratchet::new(session_id, counter, ticks_to_live);
        for _ in 0 .. 16 {
            counter += 1;
            assert!(ratchet.update(&session_id, counter));
        }

        // Can not use the old messages for replay:
        for i in 0 .. 16 {
            assert!(!ratchet.update(&session_id, i));
        }

        // Changing session_id = 1u128:
        session_id = 1u128;
        counter = 0;
        for _ in 0 .. 16 {
            counter += 1;
            assert!(ratchet.update(&session_id, counter));
        }

        // Can not use the old messages for replay:
        for i in 0 .. 16 {
            assert!(!ratchet.update(&session_id, i));
        }

        // Going back to session_id == 0u128:
        session_id = 0u128;
        counter = 0;
        for _ in 0 .. 16 {
            counter += 1;
            assert!(ratchet.update(&session_id, counter));
        }

        // Can not use the old messages for replay:
        for i in 0 .. 16 {
            assert!(!ratchet.update(&session_id, i));
        }
    }

    #[test]
    fn test_ratchet_tick() {
        let mut session_id = 0u128;
        let ticks_to_live = 8;

        let mut ratchet = Ratchet::new(session_id, 0, ticks_to_live);

        // Successful updates should reset the ratchet's `cur_ticks_to_live`:
        assert!(ratchet.update(&session_id, 1));
        for i in 0 .. 7 {
            assert_eq!(ratchet.tick(), 7 - i);
        }
        assert!(ratchet.update(&session_id, 2));
        for i in 0 .. 7 {
            assert_eq!(ratchet.tick(), 7 - i);
        }
        assert!(ratchet.update(&session_id, 3));
        for i in 0 .. 7 {
            assert_eq!(ratchet.tick(), 7 - i);
        }

        // Unsucessful update does not affect the ratchet's `cur_ticks_to_live`:
        assert!(!ratchet.update(&session_id, 3));
        assert_eq!(ratchet.tick(), 0);

        // Successful update reset's `cur_ticks_to_live`:
        assert!(ratchet.update(&session_id, 4));
        assert_eq!(ratchet.tick(), 7);
    }


    #[test]
    fn test_ratchet_pool_basic() {
        let ratchet_ticks_to_live = 8;
        let mut ratchet_pool = RatchetPool::new(ratchet_ticks_to_live);

        assert!(ratchet_pool.update(&0u128, &0u128, 0));
        assert!(ratchet_pool.update(&0u128, &0u128, 1));
        assert!(ratchet_pool.update(&0u128, &0u128, 2));

        assert!(ratchet_pool.update(&1u128, &5u128, 100));
        assert!(ratchet_pool.update(&1u128, &5u128, 101));
        assert!(ratchet_pool.update(&1u128, &5u128, 102));
        assert!(ratchet_pool.update(&1u128, &6u128, 200));
        assert!(ratchet_pool.update(&1u128, &6u128, 201));
        assert!(ratchet_pool.update(&1u128, &6u128, 202));

        assert!(ratchet_pool.update(&0u128, &0u128, 3));
        assert!(ratchet_pool.update(&0u128, &0u128, 4));
        assert!(!ratchet_pool.update(&0u128, &0u128, 3));
        assert!(!ratchet_pool.update(&0u128, &0u128, 3));
        assert!(ratchet_pool.update(&0u128, &0u128, 5));

        assert!(ratchet_pool.update(&1u128, &6u128, 203));
        assert!(ratchet_pool.update(&1u128, &6u128, 204));
        assert!(ratchet_pool.update(&1u128, &6u128, 205));
        assert!(!ratchet_pool.update(&1u128, &6u128, 205));
    }

    #[test]
    fn test_ratchet_pool_tick() {
        let ratchet_ticks_to_live = 8;
        let mut ratchet_pool = RatchetPool::new(ratchet_ticks_to_live);

        assert!(ratchet_pool.update(&0u128, &0u128, 0));
        assert!(ratchet_pool.update(&1u128, &5u128, 100));

        for _ in 0 .. 4 {
            assert_eq!(ratchet_pool.tick(), vec![]);
        }
        assert!(ratchet_pool.update(&1u128, &5u128, 101));
        for _ in 0 .. 3 {
            assert_eq!(ratchet_pool.tick(), vec![]);
        }
        assert_eq!(ratchet_pool.tick(), vec![0u128]);

        // We expect that node 0u128 was removed, 
        // but node 1u128 was not removed:

        // A proof that node 0u128 removed:
        assert!(ratchet_pool.update(&0u128, &0u128, 0));

        // A proof that node 1u128 was not removed:
        assert!(!ratchet_pool.update(&1u128, &5u128, 101));
    }
}

