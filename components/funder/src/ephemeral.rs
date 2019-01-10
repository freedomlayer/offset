use std::hash::Hash;
use super::freeze_guard::{FreezeGuard, FreezeGuardMutation};
use super::liveness::{Liveness, LivenessMutation};
use super::state::FunderState;

use common::canonical_serialize::CanonicalSerialize;

#[derive(Clone)]
pub struct Ephemeral<P:Clone> {
    pub freeze_guard: FreezeGuard<P>,
    pub liveness: Liveness<P>,
}

#[derive(Debug)]
pub enum EphemeralMutation<P> {
    LivenessMutation(LivenessMutation<P>),
    FreezeGuardMutation(FreezeGuardMutation<P>),
}

impl<P> Ephemeral<P> 
where
    P: Clone + Eq + Hash,
{
    pub fn new<A,RS,FS,MS>(funder_state: &FunderState<A,P,RS,FS,MS>) -> Self 
    where
        A: CanonicalSerialize + Clone,
        RS: Clone,
        FS: Clone,
        MS: Clone,
    {
        Ephemeral {
            freeze_guard: FreezeGuard::new(&funder_state.local_public_key)
                .load_funder_state(funder_state),
            liveness: Liveness::new(),
        }
    }

    pub fn mutate(&mut self, mutation: &EphemeralMutation<P>) {
        match mutation {
            EphemeralMutation::LivenessMutation(liveness_mutation) => 
                self.liveness.mutate(liveness_mutation),
            EphemeralMutation::FreezeGuardMutation(freeze_guard_mutation) => 
                self.freeze_guard.mutate(freeze_guard_mutation),
        }
    }
}
