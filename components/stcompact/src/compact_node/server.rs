use database::DatabaseClient;

use app::conn::AppConnTuple;

use crate::gen::GenUid;

use crate::compact_node::persist::CompactState;
use crate::compact_node::types::{CompactNodeError, ConnPairCompact};

use crate::compact_node::server_init::compact_node_init;
use crate::compact_node::server_loop::compact_node_loop;

pub async fn compact_node<CG>(
    app_conn_tuple: AppConnTuple,
    mut conn_pair_compact: ConnPairCompact,
    mut compact_state: CompactState,
    mut database_client: DatabaseClient<CompactState>,
    mut compact_gen: CG,
) -> Result<(), CompactNodeError>
where
    CG: GenUid,
{
    compact_node_init(
        &mut conn_pair_compact,
        &mut compact_state,
        &mut database_client,
        &mut compact_gen,
    )
    .await?;
    compact_node_loop(
        app_conn_tuple,
        conn_pair_compact,
        compact_state,
        database_client,
        compact_gen,
    )
    .await
}
