use super::freeze_guard::FreezeGuard;
use super::liveness::Liveness;


pub struct FunderEphemeral {
    pub freeze_guard: FreezeGuard,
    pub liveness: Liveness,
}

impl FunderEphemeral {
}
