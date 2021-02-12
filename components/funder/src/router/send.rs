use crate::router::types::{RouterControl, RouterError, RouterInfo};

/// Send all pending MoveToken messages
pub async fn flush_friends(
    control: &mut impl RouterControl,
    info: &RouterInfo,
) -> Result<(), RouterError> {
    todo!();
}
