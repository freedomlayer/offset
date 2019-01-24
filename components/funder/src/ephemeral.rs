use super::liveness::{Liveness, LivenessMutation};
use super::state::FunderState;

use common::canonical_serialize::CanonicalSerialize;

#[derive(Clone)]
pub struct Ephemeral {
    pub liveness: Liveness,
}

#[derive(Debug)]
pub enum EphemeralMutation {
    LivenessMutation(LivenessMutation),
}

impl Ephemeral {
    pub fn new<A>(funder_state: &FunderState<A>) -> Ephemeral 
    where
        A: CanonicalSerialize + Clone,
    {
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
