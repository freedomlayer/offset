use std::collections::HashMap;

struct Ratchet<U> {
    /// A randomly generated sessionId. The counter is related to this session Id.
    pub session_id: U,
    /// Incrementing counter, making sure that mutations are received in the correct order.
    /// For a new session, the counter should begin from 0 and increment by 1 for every MutationsUpdate message.
    /// When a new connection is established, a new sesionId should be randomly generated.
    pub counter: u64,
    /// This number decreases as time progresses.
    /// If it reaches 0, NodeState is removed.
    pub ticks_to_live: usize,
}

impl<U> Ratchet<U> 
where
    U: std::cmp::Eq + Clone,
{
    fn new(session_id: U,
           counter: u64,
           ticks_to_live: usize) -> Self {

        Ratchet {
            session_id,
            counter,
            ticks_to_live,
        }
    }

    fn update(&mut self, session_id: &U, counter: u64) -> bool {
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

    pub fn tick(&mut self) {
        self.ratchets.retain(|node, ratchet| {
            ratchet.ticks_to_live = ratchet.ticks_to_live.saturating_sub(1);
            ratchet.ticks_to_live > 0
        });
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
    fn test_ratchet_basic() {
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
}

