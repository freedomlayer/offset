use super::freeze_guard::FreezeGuard;
use super::liveness::Liveness;
use super::state::FunderState;

#[derive(Clone)]
pub struct FunderEphemeral {
    pub freeze_guard: FreezeGuard,
    pub liveness: Liveness,
}

impl FunderEphemeral {
    pub fn new<A: Clone>(funder_state: &FunderState<A>) -> FunderEphemeral {
        FunderEphemeral {
            freeze_guard: FreezeGuard::new(&funder_state.local_public_key)
                .load_funder_state(funder_state),
            liveness: Liveness::new(),
        }
    }
}
