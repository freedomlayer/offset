use futures::{Sink, SinkExt};

use crate::compact_node::create_compact_report;
use crate::compact_node::messages::{CompactToUser, CompactToUserAck};
use crate::compact_node::persist::CompactState;
use crate::compact_node::types::{CompactNodeError, CompactServerState};

/// Update compact state, and send compact report to user if necessary
pub async fn update_send_compact_state<US>(
    compact_state: CompactState,
    server_state: &mut CompactServerState,
    user_sender: &mut US,
) -> Result<(), CompactNodeError>
where
    US: Sink<CompactToUserAck> + Unpin,
{
    // Check if the old compact state is not the same as the new one
    if server_state.compact_state() != &compact_state {
        server_state.update_compact_state(compact_state).await?;
        let new_compact_report = create_compact_report(
            server_state.compact_state().clone(),
            server_state.node_report().clone(),
        );

        let compact_to_user = CompactToUser::Report(new_compact_report);
        user_sender
            .send(CompactToUserAck::CompactToUser(compact_to_user))
            .await
            .map_err(|_| CompactNodeError::UserSenderError)?;
    }
    Ok(())
}
