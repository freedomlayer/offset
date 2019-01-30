use super::liveness::{Liveness, LivenessMutation};

#[derive(Clone)]
pub struct Ephemeral {
    pub liveness: Liveness,
}

#[derive(Debug)]
pub enum EphemeralMutation {
    LivenessMutation(LivenessMutation),
}

impl Ephemeral {
    pub fn new() -> Ephemeral {
        Ephemeral {
            liveness: Liveness::new(),
        }
    }

    pub fn mutate(&mut self, mutation: &EphemeralMutation) {
        match mutation {
            EphemeralMutation::LivenessMutation(liveness_mutation) => 
                self.liveness.mutate(liveness_mutation),
        }
    }
}
