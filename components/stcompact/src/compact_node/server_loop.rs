use futures::{future, stream, StreamExt, channel::mpsc, SinkExt};

use common::select_streams::select_streams;
use common::conn::BoxStream;

use database::{DatabaseClient};

use app::conn::AppConnTuple;

use crate::compact_node::persist::CompactState;
use crate::compact_node::types::{CompactServerEvent, CompactServerState, CompactNodeError, ConnPairCompact};

use crate::compact_node::handle_user::handle_user;
use crate::compact_node::handle_node::handle_node;
use crate::compact_node::permission::check_permission;
use crate::gen::GenUid;


/// The compact server is mediating between the user and the node.
async fn inner_compact_node_loop<CG>(app_conn_tuple: AppConnTuple, 
    conn_pair_compact: ConnPairCompact, 
    compact_state: CompactState,
    database_client: DatabaseClient<CompactState>,
    mut compact_gen: CG,
    mut opt_event_sender: Option<mpsc::Sender<()>>) -> Result<(), CompactNodeError> 
where
    CG: GenUid,
{

    // Interaction with the user:
    let (mut user_sender, user_receiver) = conn_pair_compact.split();
    let (app_permissions, node_report, conn_pair_app) = app_conn_tuple;
    // Interaction with the offst node:
    let (mut app_sender, app_receiver) = conn_pair_app.split();

    let user_receiver = user_receiver.map(CompactServerEvent::User)
        .chain(stream::once(future::ready(CompactServerEvent::UserClosed)));

    let app_receiver = app_receiver.map(CompactServerEvent::Node)
        .chain(stream::once(future::ready(CompactServerEvent::NodeClosed)));

    let mut incoming_events = select_streams![
        user_receiver,
        app_receiver
    ];

    let mut server_state = CompactServerState::new(node_report, compact_state, database_client);

    while let Some(event) = incoming_events.next().await {
        match event {
            CompactServerEvent::User(from_user) => {
                if check_permission(&from_user.inner, &app_permissions) {
                    handle_user(from_user, &app_permissions, &mut server_state, &mut compact_gen, &mut user_sender, &mut app_sender).await?;
                } else {
                    // Operation not permitted, we close the connection
                    return Ok(());
                }
            },
            CompactServerEvent::UserClosed => return Ok(()),
            CompactServerEvent::Node(app_server_to_app) => handle_node(app_server_to_app, &mut server_state, &mut compact_gen, &mut user_sender, &mut app_sender).await?,
            CompactServerEvent::NodeClosed => return Ok(()),
        }
        if let Some(ref mut event_sender) = opt_event_sender {
            let _ = event_sender.send(()).await;
        }
    }
    Ok(())
}

pub async fn compact_node_loop<CG>(app_conn_tuple: AppConnTuple, 
    conn_pair_compact: ConnPairCompact,
    compact_state: CompactState,
    database_client: DatabaseClient<CompactState>,
    compact_gen: CG) -> Result<(), CompactNodeError> 
where   
    CG: GenUid,
{
    inner_compact_node_loop(app_conn_tuple, conn_pair_compact, compact_state, database_client, compact_gen, None).await
}
