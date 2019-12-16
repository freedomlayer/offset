use std::fmt::Debug; use std::collections::HashMap;

use futures::{stream, StreamExt, SinkExt, channel::{mpsc, oneshot}, Sink, TryFutureExt};
use futures::future;
#[allow(unused)]
use futures::task::{Spawn, SpawnExt};

use crypto::rand::CryptoRandom;

use common::select_streams::select_streams;
use common::conn::{ConnPair, BoxStream, FutTransform, ConnPairVec};

use timer::TimerClient;

#[allow(unused)]
use app::common::{derive_public_key, Uid, NetAddress};
use app::conn::ConnPairApp;

#[allow(unused)]
use proto::consts::{
    KEEPALIVE_TICKS, MAX_FRAME_LENGTH, MAX_NODE_RELAYS, MAX_OPERATIONS_IN_BATCH, TICKS_TO_REKEY,
    TICK_MS,
};

#[allow(unused)]
use node::{NodeConfig, node, IncomingAppConnection, ConnPairServer};
use proto::app_server::messages::{AppPermissions, NodeReport};

#[allow(unused)]
use crate::messages::{ServerToUser, UserToServer, ServerToUserAck, UserToServerAck, NodeId, NodeName, 
    RequestCreateNode, NodeInfo, NodeInfoLocal, NodeInfoRemote, CreateNodeLocal, CreateNodeRemote, 
    NodeStatus, NodesStatus, ResponseOpenNode};
#[allow(unused)]
use crate::compact_node::{CompactToUser, CompactToUserAck, UserToCompact, UserToCompactAck, compact_node, ConnPairCompact};
use crate::gen::{GenPrivateKey, GenCryptoRandom};
use crate::store::{Store, LoadedNode};


/// Memory allocated to a channel in memory (Used to connect two components)
const CHANNEL_LEN: usize = 0x20;
/// The amount of ticks we wait before attempting to reconnect
const BACKOFF_TICKS: usize = 0x8;
/// Maximum amount of encryption set ups (diffie hellman) that we allow to occur at the same
/// time.
const MAX_CONCURRENT_ENCRYPT: usize = 0x8;
/// The size we allocate for the user send funds requests queue.
const MAX_PENDING_USER_REQUESTS: usize = 0x20;
/// Maximum amount of concurrent index client requests:
const MAX_OPEN_INDEX_CLIENT_REQUESTS: usize = 0x8;
/// The amount of ticks we are willing to wait until a connection is established (Through
/// the relay)
const CONN_TIMEOUT_TICKS: usize = 0x8;
/// Maximum amount of concurrent applications
/// going through the incoming connection transform at the same time
const MAX_CONCURRENT_INCOMING_APPS: usize = 0x8;


pub type ConnPairCompactServer = ConnPair<ServerToUserAck, UserToServerAck>;

#[allow(unused)]
#[derive(Debug)]
pub enum ServerError { 
    UserSenderError,
    NodeSenderError,
    DerivePublicKeyError,
    StoreError,
    CreateTimerError,
    SpawnError,
    FirstNodeReportError,
    SendConnPairError,
}

type CompactNodeEvent = (NodeId, CompactToUserAck);

pub enum ServerEvent {
    User(UserToServerAck),
    UserClosed,
    CompactNode(CompactNodeEvent),
}

#[derive(Debug)]
pub struct OpenNode {
    pub node_name: NodeName,
    pub sender: mpsc::Sender<UserToCompactAck>,
    // TODO: How to signal to close the node?
    // Possibly await some handle (If we use spawn_with_handle for example?)
}

#[derive(Debug)]
pub struct ServerState<ST, R, C, S> {
    pub next_node_id: NodeId,
    pub open_nodes: HashMap<NodeId, OpenNode>,
    pub store: ST,
    pub rng: R,
    pub timer_client: TimerClient,
    pub version_connector: C, 
    pub spawner: S,
}

impl<ST,R,C,S> ServerState<ST,R,C,S> {
    pub fn new(
        store: ST, 
        rng: R, 
        timer_client: TimerClient, 
        version_connector: C, 
        spawner: S) -> Self {

        Self {
            next_node_id: NodeId(0),
            open_nodes: HashMap::new(),
            store,
            rng, 
            timer_client,
            version_connector,
            spawner,
        }
    }

    fn is_node_open(&self, node_name: &NodeName) -> bool {
        for open_node in self.open_nodes.values() {
            if &open_node.node_name == node_name {
                return true;
            }
        }
        false
    }
}

#[allow(unused)]
pub async fn handle_create_node_local<ST,CG>(
    create_node_local: CreateNodeLocal, 
    store: &mut ST, 
    compact_gen: &mut CG) -> Result<bool, ServerError> 

where
    ST: Store,
    CG: GenPrivateKey,
{
    // Randomly generate a private key ourselves, because we don't trust the user to correctly randomly
    // generate a private key.
    let node_private_key = compact_gen.gen_private_key();
    let node_public_key = derive_public_key(&node_private_key).map_err(|_| ServerError::DerivePublicKeyError)?;
    Ok(if let Err(e) = store.create_local_node(create_node_local.node_name.clone(), node_private_key).await {
        warn!("handle_create_node_local: store error: {:?}", e);
        false
    } else {
        true
    })
}

pub async fn handle_create_node_remote<ST>(
    create_node_remote: CreateNodeRemote, 
    store: &mut ST) -> Result<bool, ServerError> 
where
    ST: Store,
{
    /*
    let app_public_key = derive_public_key(&create_node_remote.app_private_key)
                    .map_err(|_| ServerError::DerivePublicKeyError)?;
    */
    let create_remote_node_res = store.create_remote_node(
        create_node_remote.node_name.clone(),
        create_node_remote.app_private_key,
        create_node_remote.node_public_key.clone(),
        create_node_remote.node_address.clone()).await;

    Ok(if let Err(e) = create_remote_node_res {
        warn!("handle_create_node_remote: store error: {:?}", e);
        false
    } else {
        true
    })
}

/// Returns if any change has happened
async fn handle_create_node<ST,R,C,S,CG>(
    request_create_node: RequestCreateNode,
    server_state: &mut ServerState<ST,R,C,S>,
    compact_gen: &mut CG) -> Result<bool, ServerError> 
where
    CG: GenPrivateKey,
    ST: Store,
{

    Ok(match request_create_node {
        RequestCreateNode::CreateNodeLocal(local) => {
            if server_state.is_node_open(&local.node_name) {
                warn!("handle_create_node: Node {:?} is already open!", local.node_name);
                return Ok(false);
            }
            handle_create_node_local(local, &mut server_state.store, compact_gen).await?
        }
        RequestCreateNode::CreateNodeRemote(remote) => {
            if server_state.is_node_open(&remote.node_name) {
                warn!("handle_create_node: Node {:?} is already open!", remote.node_name);
                return Ok(false);
            }
            handle_create_node_remote(remote, &mut server_state.store).await?
        }
    })
}

pub async fn handle_remove_node<ST,R,C,S>(
    node_name: NodeName, 
    server_state: &mut ServerState<ST,R,C,S>) -> Result<bool, ServerError> 
where
    ST: Store,
{
    // Make sure that we do not attempt to remove an open node:
    if server_state.is_node_open(&node_name) {
        warn!("handle_remove_node(): node {:?} is open", node_name);
        return Ok(false);
    }


    let remove_res = server_state.store.remove_node(node_name.clone()).await;
    Ok(if let Err(e) = remove_res {
        warn!("handle_remove_node(): store error: {:?}", e);
        false
    } else {
        true
    })
}

async fn handle_close_node<ST,R,C,S>(node_id: &NodeId, server_state: &mut ServerState<ST,R,C,S>) -> Result<bool, ServerError> 
where
    ST: Store,
{
    // Remove from open nodes:
    let open_node = if let Some(open_node) = server_state.open_nodes.remove(&node_id) {
        open_node
    } else {
        return Ok(false);
    };

    // TODO: Make sure we drop everything besides the name:
    let OpenNode {
        node_name,
        ..
    } = open_node;

    // Unload from store:
    if let Err(e) = server_state.store.unload_node(&node_name).await {
        warn!("handle_close_node(): Error in unload_node(): {:?}", e);
    }

    Ok(true)
}

#[allow(unused)]
const NODE_CONFIG: NodeConfig = NodeConfig {
    /// Memory allocated to a channel in memory (Used to connect two components)
    channel_len: CHANNEL_LEN,
    /// The amount of ticks we wait before attempting to reconnect
    backoff_ticks: BACKOFF_TICKS,
    /// The amount of ticks we wait until we decide an idle connection has timed out.
    keepalive_ticks: KEEPALIVE_TICKS,
    /// Amount of ticks to wait until the next rekeying (Channel encryption)
    ticks_to_rekey: TICKS_TO_REKEY,
    /// Maximum amount of encryption set ups (diffie hellman) that we allow to occur at the same
    /// time.
    max_concurrent_encrypt: MAX_CONCURRENT_ENCRYPT,
    /// The amount of ticks we are willing to wait until a connection is established (Through
    /// the relay)
    conn_timeout_ticks: CONN_TIMEOUT_TICKS,
    /// Maximum amount of operations in one move token message
    max_operations_in_batch: MAX_OPERATIONS_IN_BATCH,
    /// The size we allocate for the user send funds requests queue.
    max_pending_user_requests: MAX_PENDING_USER_REQUESTS,
    /// Maximum amount of concurrent index client requests:
    max_open_index_client_requests: MAX_OPEN_INDEX_CLIENT_REQUESTS,
    /// Maximum amount of relays a node may use.
    max_node_relays: MAX_NODE_RELAYS,
    /// Maximum amount of incoming app connections we set up at the same time
    max_concurrent_incoming_apps: MAX_CONCURRENT_INCOMING_APPS,
};


#[allow(unused)]
async fn handle_open_node<ST,R,C,US,S>(
    node_name: NodeName, 
    server_state: &mut ServerState<ST,R,C,S>, 
    user_sender: &mut US) -> Result<bool, ServerError> 
where
    ST: Store,
    US: Sink<ServerToUserAck> + Unpin,
    R: CryptoRandom + Clone + 'static,
    // TODO: Sync is probably not necessary here.
    // See https://github.com/rust-lang/rust/issues/57017
    S: Spawn + Clone + Send + Sync + 'static,
    C: FutTransform<Input = NetAddress, Output = Option<ConnPairVec>> + Clone + Send + 'static,
{
    // Load node from store:
    let loaded_node = match server_state.store.load_node(node_name.clone()).await {
        Ok(loaded_node) => loaded_node,
        Err(e) => {
            let response_open_node = ResponseOpenNode::Failure(node_name);
            let server_to_user = ServerToUser::ResponseOpenNode(response_open_node);
            user_sender.send(ServerToUserAck::ServerToUser(server_to_user)).await.map_err(|_| ServerError::UserSenderError)?;
            warn!("handle_open_node: load_node() error: {:?}",e);
            return Ok(false);
        }
    };

    match loaded_node {
        LoadedNode::Local(local) => {
            // TODO: Spawn a new node
            
            let app_permissions = AppPermissions {
                routes: true,
                buyer: true,
                seller: true,
                config: true,
            };
            let (report_sender, report_receiver) = oneshot::channel::<(NodeReport, oneshot::Sender<ConnPairServer<NetAddress>>)>();
            let incoming_app_connection = IncomingAppConnection {
                app_permissions: app_permissions.clone(),
                report_sender,
            };

            let incoming_apps = stream::once(future::ready(incoming_app_connection));

            let c_spawner = server_state.spawner.clone();
            let node_fut = node(
                NODE_CONFIG,
                local.node_identity_client,
                server_state.timer_client.clone(),
                local.node_state,
                local.node_db_client,
                server_state.version_connector.clone(),
                incoming_apps,
                server_state.rng.clone(),
                c_spawner.clone(),
            ).map_err(|e| {
                error!("node() error: {:?}",e);
            });

            // TODO: Keep the handle:
            server_state.spawner.spawn_with_handle(node_fut).map_err(|_| ServerError::SpawnError)?;
            assert!(false);

            let (node_report, conn_pair_sender) = report_receiver.await.map_err(|_| ServerError::FirstNodeReportError)?;

            // Channel between compact app and node:
            let (compact_sender, node_receiver) = mpsc::channel(1);
            let (node_sender, compact_receiver) = mpsc::channel(1);

            let node_conn_pair = ConnPairServer::from_raw(node_sender, node_receiver);
            conn_pair_sender.send(node_conn_pair).map_err(|_| ServerError::SendConnPairError)?;

            let conn_pair_app = ConnPairApp::from_raw(compact_sender, compact_receiver);
            let app_conn_tuple = (app_permissions, node_report, conn_pair_app);

            let compact_gen = GenCryptoRandom(server_state.rng.clone());

            let (_user_sender, compact_receiver) = mpsc::channel(1);
            let (compact_sender, _user_receiver) = mpsc::channel(1);

            let conn_pair_compact = ConnPairCompact::from_raw(compact_sender, compact_receiver);

            let _ = compact_node(
                app_conn_tuple,
                conn_pair_compact,
                local.compact_state,
                local.compact_db_client,
                compact_gen);

            // TODO:
            // - Redirect incoming node messages as node events 
            //   - Add a sender into server_state, allowing to send those events.
            // - Keep things inside the open node struct:
            //  - A handle to the spawned node
            //  - A handle to the compact node at the open node struct.
            //  - A sender, allowing to send messages to the compact node.
            //
            // - Send success message to the user, together with the first NodeReport etc.
            unimplemented!();
        },

        LoadedNode::Remote(remote) => {
            // TODO: Connect to a remote node
            unimplemented!();
        }
    }
    // TODO:
    // - Open node, should be different between local, remote
    // - Save opened node's info inside an OpenNode structures, and insert into
    // `server_state.open_nodes`
    // - Send ResponseOpenNode, with relevant first CompactReport
    unimplemented!();

}


async fn build_nodes_status<ST,R,C,S>(server_state: &mut ServerState<ST,R,C,S>) -> Result<NodesStatus, ServerError> 
where
    ST: Store,
{
    let nodes_info = server_state.store.list_nodes().await.map_err(|_| ServerError::StoreError)?;
    Ok(nodes_info.into_iter().map(|(node_name, node_info)| (node_name.clone(), NodeStatus {
        is_open: server_state.is_node_open(&node_name),
        info: node_info,
    })).collect())
}


#[allow(unused)]
pub async fn handle_user_to_server<S,ST,R,C,CG,US>(
    user_to_server_ack: UserToServerAck, 
    server_state: &mut ServerState<ST,R,C,S>,
    compact_gen: &mut CG,
    user_sender: &mut US) -> Result<(), ServerError> 
where
    // TODO: Sync is probably not necessary here.
    // See https://github.com/rust-lang/rust/issues/57017
    S: Spawn + Clone + Send + Sync + 'static,
    R: CryptoRandom + Clone + 'static,
    ST: Store,
    CG: GenPrivateKey,
    US: Sink<ServerToUserAck> + Unpin,
    C: FutTransform<Input = NetAddress, Output = Option<ConnPairVec>> + Clone + Send + 'static,
{
    let UserToServerAck {
        request_id,
        inner: user_to_server,
    } = user_to_server_ack;

    let has_changed = match user_to_server {
        UserToServer::RequestCreateNode(request_create_node) => handle_create_node(request_create_node, server_state, compact_gen).await?,
        UserToServer::RequestRemoveNode(node_name) => handle_remove_node(node_name, server_state).await?,
        UserToServer::RequestOpenNode(node_name) => handle_open_node(node_name, server_state, user_sender).await?, 
        UserToServer::RequestCloseNode(node_id) => handle_close_node(&node_id, server_state).await?,
        UserToServer::Node(node_id, user_to_compact) => {
            let node_state = if let Some(node_state) = server_state.open_nodes.get_mut(&node_id) {
                node_state
            } else {
                warn!("UserToServer::Node: nonexistent node {:?}", node_id);
                return Ok(());
            };
            let user_to_compact_ack = UserToCompactAck {
                user_request_id: request_id,
                inner: user_to_compact,
            };
            return node_state.sender.send(user_to_compact_ack).await.map_err(|_| ServerError::NodeSenderError);
        },
    };

    // We get here if not of type UserToServer::Node(...):

    // If any change happened to NodesStatus, send the new version of NodesStatus to the user:
    if has_changed {
        let nodes_status_map = build_nodes_status(server_state).await?;
        let nodes_status = ServerToUser::NodesStatus(nodes_status_map);
        let server_to_user_ack = ServerToUserAck::ServerToUser(nodes_status);
        user_sender.send(server_to_user_ack).await.map_err(|_| ServerError::NodeSenderError)?;
    }

    // Send ack to the user (In the case of UserToServer::Node, another mechanism sends the ack):
    let server_to_user_ack = ServerToUserAck::Ack(request_id);
    user_sender.send(server_to_user_ack).await.map_err(|_| ServerError::NodeSenderError)
}

#[allow(unused)]
pub async fn inner_server_loop<ST,R,C,S,CG>(
    conn_pair: ConnPairCompactServer,
    store: ST,
    mut compact_gen: CG,
    timer_client: TimerClient,
    rng: R,
    version_connector: C,
    spawner: S,
    mut opt_event_sender: Option<mpsc::Sender<()>>) -> Result<(), ServerError> 
where
    ST: Store,
    CG: GenPrivateKey,
    // TODO: Sync is probably not necessary here.
    // See https://github.com/rust-lang/rust/issues/57017
    S: Spawn + Clone + Send + Sync + 'static,
    R: CryptoRandom + Clone + 'static,
    C: FutTransform<Input = NetAddress, Output = Option<ConnPairVec>> + Clone + Send + 'static,
{

    let mut server_state = ServerState::new(store, rng, timer_client, version_connector, spawner);

    let (mut user_sender, mut user_receiver) = conn_pair.split();

    let user_receiver = user_receiver.map(ServerEvent::User)
        .chain(stream::once(future::ready(ServerEvent::UserClosed)));

    let (compact_node_sender, compact_node_receiver) = mpsc::channel(1);
    let compact_node_receiver = compact_node_receiver.map(ServerEvent::CompactNode);

    let mut incoming_events = select_streams![
        user_receiver,
        compact_node_receiver
    ];

    while let Some(event) = incoming_events.next().await {
        match event {
            ServerEvent::User(user_to_server) => {
                handle_user_to_server(user_to_server, &mut server_state, &mut compact_gen, &mut user_sender).await?;
            }
            ServerEvent::UserClosed => return Ok(()),
            ServerEvent::CompactNode(compact_node_event) => {
                let (node_id, compact_to_user_ack) = compact_node_event;
                match compact_to_user_ack {
                    CompactToUserAck::Ack(request_id) => {
                        user_sender.send(ServerToUserAck::Ack(request_id))
                            .await
                            .map_err(|_| ServerError::UserSenderError)?;
                    },
                    CompactToUserAck::CompactToUser(compact_to_user) => {
                        if server_state.open_nodes.contains_key(&node_id) {
                            let server_to_user = ServerToUser::Node(node_id, compact_to_user);
                            user_sender.send(ServerToUserAck::ServerToUser(server_to_user))
                                .await
                                .map_err(|_| ServerError::UserSenderError)?;
                        } else {
                            // The user was told that this node is closed,
                            // so we do not want to forward any more messages from this node to the user.
                            warn!("inner_server_loop: compact_node_event from a node {:?} that is closed.", node_id);
                        }
                    },
                }
            },
        }
        // For testing:
        if let Some(ref mut event_sender) = opt_event_sender {
            let _ = event_sender.send(()).await;
        }
    }
    Ok(())
}

// pub async fn server_loop() -> Result<(), ServerError> {}
