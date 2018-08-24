use super::freeze_guard::FreezeGuard;
use super::liveness::Liveness;


#[derive(Clone)]
pub struct FunderEphemeral {
    pub freeze_guard: FreezeGuard,
    pub liveness: Liveness,
}

impl FunderEphemeral {
}
