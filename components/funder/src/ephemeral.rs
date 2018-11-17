use super::freeze_guard::{FreezeGuard, FreezeGuardMutation};
use super::liveness::{Liveness, LivenessMutation};
use super::state::FunderState;

#[derive(Clone)]
pub struct Ephemeral {
    pub freeze_guard: FreezeGuard,
    pub liveness: Liveness,
}

pub enum EphemeralMutation {
    LivenessMutation(LivenessMutation),
    FreezeGuardMutation(FreezeGuardMutation),
}

impl Ephemeral {
    pub fn new<A: Clone>(funder_state: &FunderState<A>) -> Ephemeral {
        Ephemeral {
            freeze_guard: FreezeGuard::new(&funder_state.local_public_key)
                .load_funder_state(funder_state),
            liveness: Liveness::new(),
        }
    }

    pub fn mutate(&mut self, mutation: &EphemeralMutation) {
        match mutation {
            EphemeralMutation::LivenessMutation(liveness_mutation) => 
                self.liveness.mutate(liveness_mutation),
            EphemeralMutation::FreezeGuardMutation(freeze_guard_mutation) => 
                self.freeze_guard.mutate(freeze_guard_mutation),
        }
    }
}
