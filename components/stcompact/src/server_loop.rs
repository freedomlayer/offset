use std::collections::{HashMap, HashSet};
use std::fmt::Debug;
use std::mem;

use futures::future::{self, RemoteHandle};
use futures::task::{Spawn, SpawnExt};
use futures::{
    channel::{mpsc, oneshot},
    stream, FutureExt, Sink, SinkExt, StreamExt, TryFutureExt,
};

use crypto::rand::CryptoRandom;

use common::conn::{BoxStream, ConnPair, ConnPairVec, FutTransform};
use common::select_streams::select_streams;

use timer::TimerClient;

use app::common::{NetAddress, Uid};
use app::conn::ConnPairApp;
use app_client::app_connect_to_node;

use proto::consts::{KEEPALIVE_TICKS, MAX_NODE_RELAYS, MAX_OPERATIONS_IN_BATCH, TICKS_TO_REKEY};

use node::{node, ConnPairServer, IncomingAppConnection, NodeConfig};
use proto::app_server::messages::{AppPermissions, NodeReport};

use crate::messages::{
    CreateNode, CreateNodeLocal, CreateNodeRemote, NodeId, NodeMode, NodeName, NodeOpened,
    NodeStatus, NodesStatus, ServerToUser, ServerToUserAck, UserToServer, UserToServerAck,
};

use crate::compact_node::messages::{
    CompactReport, CompactToUserAck, UserToCompact, UserToCompactAck,
};
use crate::compact_node::{compact_node, create_compact_report, ConnPairCompact};
use crate::gen::{GenCryptoRandom, GenPrivateKey};
use crate::store::{
    LoadedNode, LoadedNodeLocal, LoadedNodeRemote, Store, StoreError, StoredNodeConfig,
};

use connection::{create_encrypt_keepalive, create_secure_connector};

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

pub type ConnPairCompactServer = ConnPair<ServerToUserAck, UserToServerAck>;

#[derive(Debug)]
pub enum ServerError {
    UserSenderError,
    StoreError,
    SpawnError,
    FirstNodeReportError,
    SendConnPairError,
    CreateTimerError,
}

type CompactNodeEvent = (NodeId, Option<CompactToUserAck>);

#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
enum ServerEvent {
    User(UserToServerAck),
    UserClosed,
    CompactNode(CompactNodeEvent),
    TimerTick,
    TimerClosed,
    NodeConnected(NodeName, Option<RemoteNodeOpened>),
}

#[derive(Debug)]
pub struct OpenNode {
    pub node_name: NodeName,
    /// A sender, allowing to send messages to compact node
    sender: mpsc::Sender<UserToCompactAck>,
    /// A handle for a spawned node. Will close node when dropped.
    /// Exists only if this is a local node.
    opt_node_handle: Option<RemoteHandle<()>>,
    /// A handle for a spawned compact node.
    /// Will close compact node when dropped.
    compact_node_handle: RemoteHandle<()>,
    /// Request sent to this node but were not yet acked
    pending_requests: HashSet<Uid>,
}

impl OpenNode {
    /// Send a request to compact node, and keep track of unacked requests.
    /// Returns true if successful
    pub async fn send_to_compact(
        &mut self,
        request_id: Uid,
        user_to_compact: UserToCompact,
    ) -> bool {
        let user_to_compact_ack = UserToCompactAck {
            user_request_id: request_id.clone(),
            inner: user_to_compact,
        };

        if self.sender.send(user_to_compact_ack).await.is_err() {
            return false;
        }

        // Add pending request:
        self.pending_requests.insert(request_id);

        true
    }
}

#[derive(Debug)]
pub enum PreOpenNodeStatus {
    /// Amount of ticks we wait until we attempt to reconnect
    /// to the remote node:
    Delay(usize),
    /// Currently attempting a connection
    Connecting(RemoteHandle<()>),
}

#[derive(Debug)]
pub struct PreOpenNode {
    /// Information about loaded remote node
    pub loaded_node_remote: LoadedNodeRemote,
    pub status: PreOpenNodeStatus,
}

#[derive(Debug)]
struct ServerState<ST, R, C, S> {
    next_node_id: NodeId,
    /// scheduled attempts to connect to remote nodes:
    pub pre_open_nodes: HashMap<NodeName, PreOpenNode>,
    pub open_nodes: HashMap<NodeId, OpenNode>,
    pub store: ST,
    pub ticks_to_connect: usize,
    pub event_sender: mpsc::Sender<ServerEvent>,
    pub rng: R,
    pub timer_client: TimerClient,
    pub connector: C,
    pub spawner: S,
}

impl<ST, R, C, S> ServerState<ST, R, C, S> {
    pub fn new(
        store: ST,
        ticks_to_connect: usize,
        event_sender: mpsc::Sender<ServerEvent>,
        rng: R,
        timer_client: TimerClient,
        connector: C,
        spawner: S,
    ) -> Self {
        Self {
            next_node_id: NodeId(0),
            pre_open_nodes: HashMap::new(),
            open_nodes: HashMap::new(),
            store,
            ticks_to_connect,
            event_sender,
            rng,
            timer_client,
            connector,
            spawner,
        }
    }

    fn create_node_mode(&self, node_name: &NodeName) -> NodeMode {
        for (node_id, open_node) in &self.open_nodes {
            if &open_node.node_name == node_name {
                return NodeMode::Open(node_id.clone());
            }
        }
        NodeMode::Closed
    }

    /// Get a new unique node_id
    fn new_node_id(&mut self) -> NodeId {
        let next_node_id = self.next_node_id.clone();
        // Advance `next_node_id`, to make sure next time we get a different NodeId:
        self.next_node_id.0 = self.next_node_id.0.wrapping_add(1);
        next_node_id
    }
}

async fn is_node_enabled<ST>(node_name: &NodeName, store: &ST) -> Result<bool, ServerError>
where
    ST: Store,
{
    let stored_nodes = store
        .list_nodes()
        .await
        .map_err(|_| ServerError::StoreError)?;
    if let Some(stored_node) = stored_nodes.get(node_name) {
        Ok(stored_node.config.is_enabled)
    } else {
        Ok(false)
    }
}

async fn is_node_exists<ST>(node_name: &NodeName, store: &ST) -> Result<bool, ServerError>
where
    ST: Store,
{
    let stored_nodes = store
        .list_nodes()
        .await
        .map_err(|_| ServerError::StoreError)?;
    if stored_nodes.get(node_name).is_some() {
        Ok(true)
    } else {
        Ok(false)
    }
}

async fn build_nodes_status<ST, R, C, S>(
    server_state: &ServerState<ST, R, C, S>,
) -> Result<NodesStatus, ServerError>
where
    ST: Store,
{
    let stored_nodes = server_state
        .store
        .list_nodes()
        .await
        .map_err(|_| ServerError::StoreError)?;
    Ok(stored_nodes
        .into_iter()
        .map(|(node_name, stored_node)| {
            (
                node_name.clone(),
                NodeStatus {
                    mode: server_state.create_node_mode(&node_name),
                    is_enabled: stored_node.config.is_enabled,
                    info: stored_node.info,
                },
            )
        })
        .collect())
}

async fn send_nodes_status_ack<US>(
    opt_nodes_status: Option<NodesStatus>,
    request_id: Uid,
    user_sender: &mut US,
) -> Result<(), ServerError>
where
    US: Sink<ServerToUserAck> + Unpin,
{
    if let Some(nodes_status) = opt_nodes_status {
        let server_to_user = ServerToUser::NodesStatus(nodes_status);
        let server_to_user_ack = ServerToUserAck::ServerToUser(server_to_user);
        user_sender
            .send(server_to_user_ack)
            .await
            .map_err(|_| ServerError::UserSenderError)?;
    }

    // Send ack to the user (In the case of UserToServer::Node, another mechanism sends the ack):
    let server_to_user_ack = ServerToUserAck::Ack(request_id);
    user_sender
        .send(server_to_user_ack)
        .await
        .map_err(|_| ServerError::UserSenderError)
}

async fn handle_create_node_local<ST, CG>(
    create_node_local: CreateNodeLocal,
    store: &mut ST,
    compact_gen: &mut CG,
) -> Result<bool, ServerError>
where
    ST: Store,
    CG: GenPrivateKey,
{
    // Randomly generate a private key ourselves, because we don't trust the user to correctly randomly
    // generate a private key.
    let node_private_key = compact_gen.gen_private_key();
    Ok(
        if let Err(e) = store
            .create_local_node(create_node_local.node_name.clone(), node_private_key)
            .await
        {
            warn!("handle_create_node_local: store error: {:?}", e);
            if e.is_fatal() {
                return Err(ServerError::StoreError);
            }
            false
        } else {
            true
        },
    )
}

async fn handle_create_node_remote<ST>(
    create_node_remote: CreateNodeRemote,
    store: &mut ST,
) -> Result<bool, ServerError>
where
    ST: Store,
{
    let create_remote_node_res = store
        .create_remote_node(
            create_node_remote.node_name.clone(),
            create_node_remote.app_private_key,
            create_node_remote.node_public_key.clone(),
            create_node_remote.node_address.clone(),
        )
        .await;

    Ok(if let Err(e) = create_remote_node_res {
        warn!("handle_create_node_remote: store error: {:?}", e);
        false
    } else {
        true
    })
}

/// Returns if any change has happened
async fn handle_create_node<ST, R, C, S, CG, US>(
    request_create_node: CreateNode,
    server_state: &mut ServerState<ST, R, C, S>,
    compact_gen: &mut CG,
    request_id: Uid,
    user_sender: &mut US,
) -> Result<(), ServerError>
where
    CG: GenPrivateKey,
    ST: Store,
    US: Sink<ServerToUserAck> + Unpin,
{
    let has_changed = match request_create_node {
        CreateNode::CreateNodeLocal(local) => {
            if is_node_exists(&local.node_name, &server_state.store).await? {
                warn!(
                    "handle_create_node: A node named {:?} already exists!",
                    local.node_name
                );
                false
            } else {
                handle_create_node_local(local, &mut server_state.store, compact_gen).await?
            }
        }
        CreateNode::CreateNodeRemote(remote) => {
            if is_node_exists(&remote.node_name, &server_state.store).await? {
                warn!(
                    "handle_create_node: A node named {:?} already exists!",
                    remote.node_name
                );
                false
            } else {
                handle_create_node_remote(remote, &mut server_state.store).await?
            }
        }
    };

    let nodes_status = build_nodes_status(&server_state).await?;
    send_nodes_status_ack(
        Some(nodes_status).filter(|_| has_changed),
        request_id,
        user_sender,
    )
    .await
}

async fn handle_remove_node<ST, R, C, S, US>(
    node_name: NodeName,
    server_state: &mut ServerState<ST, R, C, S>,
    request_id: Uid,
    user_sender: &mut US,
) -> Result<(), ServerError>
where
    ST: Store,
    US: Sink<ServerToUserAck> + Unpin,
{
    // Make sure that we do not attempt to remove an open node:
    let has_changed = if is_node_enabled(&node_name, &server_state.store).await? {
        warn!("handle_remove_node(): node {:?} is enabled", node_name);
        false
    } else {
        let remove_res = server_state.store.remove_node(node_name.clone()).await;
        if let Err(e) = remove_res {
            warn!("handle_remove_node(): store error: {:?}", e);
            if e.is_fatal() {
                return Err(ServerError::StoreError);
            }
            false
        } else {
            true
        }
    };

    let nodes_status = build_nodes_status(&server_state).await?;
    send_nodes_status_ack(
        Some(nodes_status).filter(|_| has_changed),
        request_id,
        user_sender,
    )
    .await
}

/// Remove a node from memory, unload it and notify user
async fn close_node_and_notify_user<ST, R, C, S, US>(
    node_id: &NodeId,
    server_state: &mut ServerState<ST, R, C, S>,
    user_sender: &mut US,
) -> Result<(), ServerError>
where
    ST: Store,
    US: Sink<ServerToUserAck> + Unpin,
{
    // Remove from open nodes:
    let node_name = if let Some(open_node) = server_state.open_nodes.remove(&node_id) {
        // TODO: Make sure we drop everything besides the name:
        let OpenNode {
            node_name,
            pending_requests,
            ..
        } = open_node;

        // Unload from store:
        if let Err(e) = server_state.store.unload_node(&node_name).await {
            warn!(
                "handle_close_node_and_notify_user(): Error in unload_node(): {:?}",
                e
            );
            if e.is_fatal() {
                return Err(ServerError::StoreError);
            }
        }

        // Notify user about changes:
        let nodes_status = build_nodes_status(&server_state).await?;
        let server_to_user = ServerToUser::NodesStatus(nodes_status);
        let server_to_user_ack = ServerToUserAck::ServerToUser(server_to_user);
        user_sender
            .send(server_to_user_ack)
            .await
            .map_err(|_| ServerError::UserSenderError)?;

        // Send acks for all open the requests that were left un-acked:
        for request_id in pending_requests {
            let server_to_user_ack = ServerToUserAck::Ack(request_id);
            user_sender
                .send(server_to_user_ack)
                .await
                .map_err(|_| ServerError::UserSenderError)?;
        }

        node_name
    } else {
        // We don't know which node was closed.
        // We don't have a record of this node anymore.
        return Ok(());
    };

    // Load node from store:
    let loaded_node = match server_state.store.load_node(node_name.clone()).await {
        Ok(loaded_node) => loaded_node,
        Err(e) => {
            warn!("close_node_and_notify_user(): load_node() error: {:?}", e);
            if e.is_fatal() {
                return Err(ServerError::StoreError);
            }
            return Ok(());
        }
    };

    match loaded_node {
        LoadedNode::Local(_loaded_node_local) => {
            // Local nodes should only be closed intentionaly
            // We unload the node.
            // TODO: Possibly make this more efficient. Maybe we can detect earlier that this is a
            // local node, instead of loading and unloading it.
            server_state
                .store
                .unload_node(&node_name)
                .await
                .map_err(|_| ServerError::StoreError)?;
        }
        LoadedNode::Remote(loaded_node_remote) => {
            // Schedule attempt to open node
            let pre_open_node = PreOpenNode {
                loaded_node_remote,
                status: PreOpenNodeStatus::Delay(0),
            };
            server_state.pre_open_nodes.insert(node_name, pre_open_node);
        }
    }
    Ok(())
}

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
};

async fn open_node_local<ST, R, C, S>(
    node_name: NodeName,
    local: LoadedNodeLocal,
    server_state: &mut ServerState<ST, R, C, S>,
) -> Result<NodeOpened, ServerError>
where
    ST: Store,
    R: CryptoRandom + Clone + 'static,
    // TODO: Sync is probably not necessary here.
    // See https://github.com/rust-lang/rust/issues/57017
    S: Spawn + Clone + Send + Sync + 'static,
    C: FutTransform<Input = NetAddress, Output = Option<ConnPairVec>> + Clone + Send + 'static,
{
    let app_permissions = AppPermissions {
        routes: true,
        buyer: true,
        seller: true,
        config: true,
    };
    let (report_sender, report_receiver) =
        oneshot::channel::<(NodeReport, oneshot::Sender<ConnPairServer<NetAddress>>)>();
    let incoming_app_connection = IncomingAppConnection {
        app_permissions: app_permissions.clone(),
        report_sender,
    };

    let incoming_apps = stream::once(future::ready(incoming_app_connection));

    let secure_connector = create_secure_connector(
        server_state.connector.clone(),
        server_state.timer_client.clone(),
        local.node_identity_client.clone(),
        server_state.rng.clone(),
        server_state.spawner.clone(),
    );

    let encrypt_keepalive = create_encrypt_keepalive(
        server_state.timer_client.clone(),
        local.node_identity_client.clone(),
        server_state.rng.clone(),
        server_state.spawner.clone(),
    );

    let node_fut = node(
        NODE_CONFIG,
        local.node_identity_client,
        server_state.timer_client.clone(),
        local.node_state,
        local.node_db_client,
        secure_connector,
        encrypt_keepalive,
        incoming_apps,
        server_state.rng.clone(),
        server_state.spawner.clone(),
    )
    .map_err(|e| {
        error!("node() error: {:?}", e);
    })
    .map(|_| ());

    let node_handle = server_state
        .spawner
        .spawn_with_handle(node_fut)
        .map_err(|_| ServerError::SpawnError)?;

    let (node_report, conn_pair_sender) = report_receiver
        .await
        .map_err(|_| ServerError::FirstNodeReportError)?;

    // Channel between compact app and node:
    let (compact_sender, node_receiver) = mpsc::channel(1);
    let (node_sender, compact_receiver) = mpsc::channel(1);

    let node_conn_pair = ConnPairServer::from_raw(node_sender, node_receiver);
    conn_pair_sender
        .send(node_conn_pair)
        .map_err(|_| ServerError::SendConnPairError)?;

    let conn_pair_app = ConnPairApp::from_raw(compact_sender, compact_receiver);
    let app_conn_tuple = (app_permissions.clone(), node_report.clone(), conn_pair_app);

    let compact_gen = GenCryptoRandom(server_state.rng.clone());

    let (local_sender, compact_receiver) = mpsc::channel(1);
    let (compact_sender, mut local_receiver) = mpsc::channel(1);

    let conn_pair_compact = ConnPairCompact::from_raw(compact_sender, compact_receiver);

    let compact_report = create_compact_report(local.compact_state.clone(), node_report);

    let compact_node_fut = compact_node(
        app_conn_tuple,
        conn_pair_compact,
        local.compact_state,
        local.compact_db_client,
        compact_gen,
    )
    .map_err(|e| {
        error!("open_node_local(): compact_node() error: {:?}", e);
    })
    .map(|_| ());

    let node_id = server_state.new_node_id();
    let compact_node_handle = server_state
        .spawner
        .spawn_with_handle(compact_node_fut)
        .map_err(|_| ServerError::SpawnError)?;

    // Keep things inside the open node struct:
    let open_node = OpenNode {
        node_name: node_name.clone(),
        sender: local_sender,
        opt_node_handle: Some(node_handle),
        compact_node_handle,
        pending_requests: HashSet::new(),
    };

    let old_value = server_state.open_nodes.insert(node_id.clone(), open_node);
    // It shouldn't be possible to have two identical node_id-s
    assert!(old_value.is_none());

    // Redirect incoming node messages as events to main loop:
    let mut c_event_sender = server_state.event_sender.clone();
    let c_node_id = node_id.clone();
    server_state
        .spawner
        .spawn(async move {
            while let Some(compact_node_event) = local_receiver.next().await {
                if let Err(e) = c_event_sender
                    .send(ServerEvent::CompactNode((
                        c_node_id.clone(),
                        Some(compact_node_event),
                    )))
                    .await
                {
                    warn!("open_node_local(): c_event_sender error! {:?}", e);
                    return;
                }
            }
            // Finally send a close event:
            if let Err(e) = c_event_sender
                .send(ServerEvent::CompactNode((c_node_id.clone(), None)))
                .await
            {
                warn!(
                    "open_node_local(): c_event_sender send close error! {:?}",
                    e
                );
            }
        })
        .map_err(|_| ServerError::SpawnError)?;

    // Send success message to the user, together with the first NodeReport etc.
    Ok(NodeOpened {
        node_name,
        node_id,
        app_permissions,
        compact_report,
    })
}

#[derive(Debug)]
struct RemoteNodeOpened {
    pub node_name: NodeName,
    pub app_permissions: AppPermissions,
    pub compact_report: CompactReport,
    pub local_sender: mpsc::Sender<UserToCompactAck>,
    pub local_receiver: mpsc::Receiver<CompactToUserAck>,
    pub compact_node_handle: RemoteHandle<()>,
}

async fn open_node_remote<R, C, S>(
    node_name: NodeName,
    remote: LoadedNodeRemote,
    connector: C,
    timer_client: TimerClient,
    rng: R,
    spawner: S,
) -> Result<Option<RemoteNodeOpened>, ServerError>
where
    R: CryptoRandom + Clone + 'static,
    // TODO: Sync is probably not necessary here.
    // See https://github.com/rust-lang/rust/issues/57017
    S: Spawn + Clone + Send + Sync + 'static,
    C: FutTransform<Input = NetAddress, Output = Option<ConnPairVec>> + Clone + Send + 'static,
{
    let secure_connector = create_secure_connector(
        connector.clone(),
        timer_client.clone(),
        remote.app_identity_client.clone(),
        rng.clone(),
        spawner.clone(),
    );

    // Connect to remote node
    let connect_res = app_connect_to_node(
        secure_connector,
        remote.node_public_key,
        remote.node_address,
        spawner.clone(),
    )
    .await;

    let (app_permissions, node_report, conn_pair) = if let Ok(tup) = connect_res {
        tup
    } else {
        // Connection failed:
        return Ok(None);
    };

    let compact_gen = GenCryptoRandom(rng.clone());

    let (local_sender, compact_receiver) = mpsc::channel(1);
    let (compact_sender, local_receiver) = mpsc::channel(1);

    let conn_pair_compact = ConnPairCompact::from_raw(compact_sender, compact_receiver);

    let compact_report = create_compact_report(remote.compact_state.clone(), node_report.clone());

    let app_conn_tuple = (app_permissions.clone(), node_report, conn_pair);

    let compact_node_fut = compact_node(
        app_conn_tuple,
        conn_pair_compact,
        remote.compact_state,
        remote.compact_db_client,
        compact_gen,
    )
    .map_err(|e| {
        error!("open_node_remote(): compact_node() error: {:?}", e);
    })
    .map(|_| ());

    let compact_node_handle = spawner
        .spawn_with_handle(compact_node_fut)
        .map_err(|_| ServerError::SpawnError)?;

    // Keep things inside the open node struct:
    Ok(Some(RemoteNodeOpened {
        node_name: node_name.clone(),
        app_permissions,
        compact_report,
        local_sender,
        local_receiver,
        compact_node_handle,
    }))
}

async fn handle_enable_node<ST, R, C, S, US>(
    node_name: NodeName,
    server_state: &mut ServerState<ST, R, C, S>,
    request_id: Uid,
    user_sender: &mut US,
) -> Result<(), ServerError>
where
    ST: Store,
    US: Sink<ServerToUserAck> + Unpin,
    R: CryptoRandom + Clone + 'static,
    // TODO: Sync is probably not necessary here.
    // See https://github.com/rust-lang/rust/issues/57017
    S: Spawn + Clone + Send + Sync + 'static,
    C: FutTransform<Input = NetAddress, Output = Option<ConnPairVec>> + Clone + Send + 'static,
{
    let stored_nodes = server_state
        .store
        .list_nodes()
        .await
        .map_err(|_| ServerError::StoreError)?;

    // Check if the node exists and is already enabled:
    match stored_nodes.get(&node_name) {
        Some(stored_node) if !stored_node.config.is_enabled => {}
        _ => {
            // Send ack:
            user_sender
                .send(ServerToUserAck::Ack(request_id))
                .await
                .map_err(|_| ServerError::UserSenderError)?;
            return Ok(());
        }
    };

    // Configure node to be enabled:
    server_state
        .store
        .config_node(node_name.clone(), StoredNodeConfig { is_enabled: true })
        .await
        .map_err(|_| ServerError::StoreError)?;

    // Load node from store:
    let loaded_node = match server_state.store.load_node(node_name.clone()).await {
        Ok(loaded_node) => loaded_node,
        Err(e) => {
            warn!("handle_enable_node: load_node() error: {:?}", e);
            if e.is_fatal() {
                return Err(ServerError::StoreError);
            }
            send_nodes_status_ack(None, request_id, user_sender).await?;

            return Ok(());
        }
    };

    // Send nodes status list and ack:
    let nodes_status = build_nodes_status(&server_state).await?;
    send_nodes_status_ack(Some(nodes_status), request_id, user_sender).await?;

    match loaded_node {
        LoadedNode::Local(loaded_node_local) => {
            // Open node
            let node_opened = open_node_local(node_name, loaded_node_local, server_state).await?;

            // Send NodeOpened:
            let server_to_user = ServerToUser::NodeOpened(node_opened);
            user_sender
                .send(ServerToUserAck::ServerToUser(server_to_user))
                .await
                .map_err(|_| ServerError::UserSenderError)?;
        }
        LoadedNode::Remote(loaded_node_remote) => {
            // Schedule attempt to open node
            let pre_open_node = PreOpenNode {
                loaded_node_remote,
                status: PreOpenNodeStatus::Delay(0),
            };
            server_state.pre_open_nodes.insert(node_name, pre_open_node);
        }
    }
    Ok(())
}

/// Open all enabled nodes (Enabled according to configuration).
/// This will usually happen during startup.
async fn open_enabled_nodes<ST, R, C, S, US>(
    server_state: &mut ServerState<ST, R, C, S>,
    user_sender: &mut US,
) -> Result<(), ServerError>
where
    ST: Store,
    US: Sink<ServerToUserAck> + Unpin,
    R: CryptoRandom + Clone + 'static,
    // TODO: Sync is probably not necessary here.
    // See https://github.com/rust-lang/rust/issues/57017
    S: Spawn + Clone + Send + Sync + 'static,
    C: FutTransform<Input = NetAddress, Output = Option<ConnPairVec>> + Clone + Send + 'static,
{
    let stored_nodes = server_state
        .store
        .list_nodes()
        .await
        .map_err(|_| ServerError::StoreError)?;

    for (node_name, stored_node) in stored_nodes {
        // Only open enabled nodes:
        if !stored_node.config.is_enabled {
            continue;
        }

        // Load node from store:
        let loaded_node = server_state
            .store
            .load_node(node_name.clone())
            .await
            .map_err(|_| ServerError::StoreError)?;

        match loaded_node {
            LoadedNode::Local(loaded_node_local) => {
                // Open node
                let node_opened =
                    open_node_local(node_name, loaded_node_local, server_state).await?;

                // Send NodeOpened:
                let server_to_user = ServerToUser::NodeOpened(node_opened);
                user_sender
                    .send(ServerToUserAck::ServerToUser(server_to_user))
                    .await
                    .map_err(|_| ServerError::UserSenderError)?;
            }
            LoadedNode::Remote(loaded_node_remote) => {
                // Schedule attempt to open node
                let pre_open_node = PreOpenNode {
                    loaded_node_remote,
                    status: PreOpenNodeStatus::Delay(0),
                };
                server_state.pre_open_nodes.insert(node_name, pre_open_node);
            }
        }
    }
    Ok(())
}

async fn handle_disable_node<ST, R, C, S, US>(
    node_name: NodeName,
    server_state: &mut ServerState<ST, R, C, S>,
    request_id: Uid,
    user_sender: &mut US,
) -> Result<(), ServerError>
where
    ST: Store,
    US: Sink<ServerToUserAck> + Unpin,
{
    // Keep `nodes_status` before we make any modifications:
    let old_nodes_status = build_nodes_status(&server_state).await?;

    // Check if the node exists and is already disabled:
    match old_nodes_status.get(&node_name) {
        Some(node_status) if node_status.is_enabled => {}
        _ => {
            // Node is already disabled:
            // Send ack:
            user_sender
                .send(ServerToUserAck::Ack(request_id))
                .await
                .map_err(|_| ServerError::UserSenderError)?;
            return Ok(());
        }
    };

    // Unload node:
    if let Err(e) = server_state.store.unload_node(&node_name).await {
        warn!("handle_disable_node(): Error in unload_node(): {:?}", e);
        if e.is_fatal() {
            return Err(ServerError::StoreError);
        }
    }

    // Configure node to be disabled:
    server_state
        .store
        .config_node(node_name.clone(), StoredNodeConfig { is_enabled: false })
        .await
        .map_err(|_| ServerError::StoreError)?;

    // If there is a scheduled task to connect to the node, remove it:
    let _ = server_state.pre_open_nodes.remove(&node_name);

    // Remove open nodes carrying this node's name, if exist:
    server_state
        .open_nodes
        .retain(|_node_id, open_node| open_node.node_name != node_name);

    let new_nodes_status = build_nodes_status(&server_state).await?;

    // If any change occured to `nodes_status`, we send `new_nodes_status`:
    if old_nodes_status != new_nodes_status {
        let server_to_user = ServerToUser::NodesStatus(new_nodes_status);
        let server_to_user_ack = ServerToUserAck::ServerToUser(server_to_user);
        user_sender
            .send(server_to_user_ack)
            .await
            .map_err(|_| ServerError::UserSenderError)?;
    }

    // Send ack:
    user_sender
        .send(ServerToUserAck::Ack(request_id))
        .await
        .map_err(|_| ServerError::UserSenderError)?;

    Ok(())
}

async fn handle_user_to_server<S, ST, R, C, CG, US>(
    user_to_server_ack: UserToServerAck,
    server_state: &mut ServerState<ST, R, C, S>,
    compact_gen: &mut CG,
    user_sender: &mut US,
) -> Result<(), ServerError>
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

    match user_to_server {
        UserToServer::CreateNode(request_create_node) => {
            handle_create_node(
                request_create_node,
                server_state,
                compact_gen,
                request_id,
                user_sender,
            )
            .await?
        }
        UserToServer::RemoveNode(node_name) => {
            handle_remove_node(node_name, server_state, request_id, user_sender).await?
        }
        UserToServer::EnableNode(node_name) => {
            handle_enable_node(node_name, server_state, request_id, user_sender).await?
        }
        UserToServer::DisableNode(node_name) => {
            handle_disable_node(node_name, server_state, request_id, user_sender).await?
        }
        UserToServer::Node(node_id, user_to_compact) => {
            let open_node = if let Some(open_node) = server_state.open_nodes.get_mut(&node_id) {
                open_node
            } else {
                warn!("UserToServer::Node: nonexistent node {:?}", node_id);

                // Send ack:
                let opt_nodes_status = None;
                send_nodes_status_ack(opt_nodes_status, request_id, user_sender).await?;
                return Ok(());
            };

            /*
            let user_to_compact_ack = UserToCompactAck {
                user_request_id: request_id.clone(),
                inner: user_to_compact,
            };
            */
            if !open_node
                .send_to_compact(request_id.clone(), user_to_compact)
                .await
            {
                warn!(
                    "UserToServer::Node: Failed to send a message to node {}",
                    open_node.node_name.as_str()
                );
                close_node_and_notify_user(&node_id, server_state, user_sender).await?;

                // Finally, acknowledge the request, so that the user will not wait forever:
                let opt_nodes_status = None;
                send_nodes_status_ack(opt_nodes_status, request_id, user_sender).await?;
            }
        }
    };

    Ok(())
}

async fn handle_timer_tick<S, ST, R, C>(
    server_state: &mut ServerState<ST, R, C, S>,
) -> Result<(), ServerError>
where
    // TODO: Sync is probably not necessary here.
    // See https://github.com/rust-lang/rust/issues/57017
    S: Spawn + Clone + Send + Sync + 'static,
    R: CryptoRandom + Clone + 'static,
    ST: Store,
    C: FutTransform<Input = NetAddress, Output = Option<ConnPairVec>> + Clone + Send + 'static,
{
    let pre_open_nodes = mem::replace(&mut server_state.pre_open_nodes, HashMap::new());

    for (node_name, pre_open_node) in pre_open_nodes {
        let status = match pre_open_node.status {
            PreOpenNodeStatus::Delay(mut ticks_to_connect) => {
                ticks_to_connect = ticks_to_connect.saturating_sub(1);
                if ticks_to_connect == 0 {
                    // Spawn a connection attempt to remote node:
                    let mut c_event_sender = server_state.event_sender.clone();
                    let c_connector = server_state.connector.clone();
                    let c_timer_client = server_state.timer_client.clone();
                    let c_rng = server_state.rng.clone();
                    let c_spawner = server_state.spawner.clone();
                    let c_loaded_node_remote = pre_open_node.loaded_node_remote.clone();
                    let c_node_name = node_name.clone();
                    let connect_handle = server_state
                        .spawner
                        .spawn_with_handle(async move {
                            let res = open_node_remote(
                                c_node_name.clone(),
                                c_loaded_node_remote,
                                c_connector,
                                c_timer_client,
                                c_rng,
                                c_spawner,
                            )
                            .await;

                            match res {
                                Ok(Some(remote_node_opened)) => {
                                    // Send event about successful connection
                                    if let Err(e) = c_event_sender
                                        .send(ServerEvent::NodeConnected(
                                            c_node_name,
                                            Some(remote_node_opened),
                                        ))
                                        .await
                                    {
                                        warn!("c_event_sender.send() error: {:?}", e);
                                    }
                                }
                                Ok(None) => {
                                    // Send event about failure to connect
                                    if let Err(e) = c_event_sender
                                        .send(ServerEvent::NodeConnected(c_node_name, None))
                                        .await
                                    {
                                        warn!("c_event_sender.send() error: {:?}", e);
                                    }
                                }
                                Err(e) => {
                                    warn!("open_node_remote() error: {:?}", e);
                                    // Send event about failure to connect
                                    if let Err(e) = c_event_sender
                                        .send(ServerEvent::NodeConnected(c_node_name, None))
                                        .await
                                    {
                                        warn!("c_event_sender.send() error: {:?}", e);
                                    }
                                }
                            }
                        })
                        .map_err(|_| ServerError::SpawnError)?;
                    PreOpenNodeStatus::Connecting(connect_handle)
                } else {
                    PreOpenNodeStatus::Delay(ticks_to_connect)
                }
            }
            PreOpenNodeStatus::Connecting(remote_handle) => {
                PreOpenNodeStatus::Connecting(remote_handle)
            }
        };

        let pre_open_node = PreOpenNode {
            loaded_node_remote: pre_open_node.loaded_node_remote,
            status,
        };

        server_state.pre_open_nodes.insert(node_name, pre_open_node);
    }

    Ok(())
}

async fn handle_node_connected<S, ST, R, C, US>(
    server_state: &mut ServerState<ST, R, C, S>,
    node_name: NodeName,
    opt_remote_node_opened: Option<RemoteNodeOpened>,
    user_sender: &mut US,
) -> Result<(), ServerError>
where
    S: Spawn + Clone + Send + Sync + 'static,
    US: Sink<ServerToUserAck> + Unpin,
{
    let pre_open_node = if let Some(pre_open_node) = server_state.pre_open_nodes.remove(&node_name)
    {
        pre_open_node
    } else {
        warn!("handle_node_opened(): Unrecognized node {:?}", node_name);
        return Ok(());
    };

    let remote_node_opened = if let Some(remote_node_opened) = opt_remote_node_opened {
        // Conection attempt succeeded:
        remote_node_opened
    } else {
        // Conection attempt failed:
        let mut pre_open_node = pre_open_node;
        // Reset delay:
        pre_open_node.status = PreOpenNodeStatus::Delay(server_state.ticks_to_connect);

        let old_value = server_state
            .pre_open_nodes
            .insert(node_name.clone(), pre_open_node);
        assert!(old_value.is_none());
        return Ok(());
    };

    // Connection was successful:

    let open_node = OpenNode {
        node_name: node_name.clone(),
        sender: remote_node_opened.local_sender,
        // Remote nodes do not need a node handle:
        opt_node_handle: None,
        compact_node_handle: remote_node_opened.compact_node_handle,
        pending_requests: HashSet::new(),
    };

    let node_id = server_state.new_node_id();
    let old_value = server_state.open_nodes.insert(node_id.clone(), open_node);
    // It shouldn't be possible to have two identical node_id-s
    assert!(old_value.is_none());

    // Receiver for messages from node:
    let mut local_receiver = remote_node_opened.local_receiver;

    // Redirect incoming node messages as events to main loop:
    let mut c_event_sender = server_state.event_sender.clone();
    let c_node_id = node_id.clone();
    server_state
        .spawner
        .spawn(async move {
            while let Some(compact_node_event) = local_receiver.next().await {
                if let Err(e) = c_event_sender
                    .send(ServerEvent::CompactNode((
                        c_node_id.clone(),
                        Some(compact_node_event),
                    )))
                    .await
                {
                    warn!("c_event_sender error! {:?}", e);
                    return;
                }
            }
            // Finally send a close event:
            if let Err(e) = c_event_sender
                .send(ServerEvent::CompactNode((c_node_id.clone(), None)))
                .await
            {
                warn!("c_event_sender send close error! {:?}", e);
            }
        })
        .map_err(|_| ServerError::SpawnError)?;

    // Send NodeOpened:
    let node_opened = NodeOpened {
        node_name,
        node_id,
        app_permissions: remote_node_opened.app_permissions,
        compact_report: remote_node_opened.compact_report,
    };

    let server_to_user = ServerToUser::NodeOpened(node_opened);
    user_sender
        .send(ServerToUserAck::ServerToUser(server_to_user))
        .await
        .map_err(|_| ServerError::UserSenderError)?;

    Ok(())
}

async fn inner_server_loop<ST, R, C, S>(
    conn_pair: ConnPairCompactServer,
    store: ST,
    ticks_to_connect: usize,
    mut timer_client: TimerClient,
    rng: R,
    connector: C,
    spawner: S,
    // opt_event_sender is used for testing:
    mut opt_event_sender: Option<mpsc::Sender<()>>,
) -> Result<(), ServerError>
where
    ST: Store,
    // TODO: Sync is probably not necessary here.
    // See https://github.com/rust-lang/rust/issues/57017
    S: Spawn + Clone + Send + Sync + 'static,
    R: CryptoRandom + Clone + 'static,
    C: FutTransform<Input = NetAddress, Output = Option<ConnPairVec>> + Clone + Send + 'static,
{
    let timer_stream = timer_client
        .request_timer_stream("inner_server_loop".to_owned())
        .await
        .map_err(|_| ServerError::CreateTimerError)?
        .map(|_| ServerEvent::TimerTick)
        .chain(stream::once(future::ready(ServerEvent::TimerClosed)));

    let (mut user_sender, user_receiver) = conn_pair.split();

    let user_receiver = user_receiver
        .map(ServerEvent::User)
        .chain(stream::once(future::ready(ServerEvent::UserClosed)));

    let (compact_node_sender, compact_node_receiver) = mpsc::channel(1);

    let mut server_state = ServerState::new(
        store,
        ticks_to_connect,
        compact_node_sender,
        rng.clone(),
        timer_client,
        connector,
        spawner,
    );

    // Send initial NodesStatus:
    // TODO: A cleaner way to do this?
    let nodes_status_map = build_nodes_status(&server_state).await?;
    let nodes_status = ServerToUser::NodesStatus(nodes_status_map);
    let server_to_user_ack = ServerToUserAck::ServerToUser(nodes_status);
    user_sender
        .send(server_to_user_ack)
        .await
        .map_err(|_| ServerError::UserSenderError)?;

    // Load all nodes that are enabled by configuration:
    open_enabled_nodes(&mut server_state, &mut user_sender).await?;

    // TODO: This is a hack, find a better solution later:
    let mut compact_gen = GenCryptoRandom(rng);

    let mut incoming_events = select_streams![user_receiver, compact_node_receiver, timer_stream];

    while let Some(event) = incoming_events.next().await {
        match event {
            ServerEvent::User(user_to_server) => {
                handle_user_to_server(
                    user_to_server,
                    &mut server_state,
                    &mut compact_gen,
                    &mut user_sender,
                )
                .await?;
            }
            ServerEvent::UserClosed => return Ok(()),
            ServerEvent::CompactNode((node_id, None)) => {
                // Node was just closed:
                // Remove from open nodes (If present):
                close_node_and_notify_user(&node_id, &mut server_state, &mut user_sender).await?;
            }
            ServerEvent::CompactNode((node_id, Some(compact_to_user_ack))) => {
                // A message from a node:
                match compact_to_user_ack {
                    CompactToUserAck::Ack(request_id) => {
                        if let Some(open_node) = server_state.open_nodes.get_mut(&node_id) {
                            let res = open_node.pending_requests.remove(&request_id);
                            // Make sure that we had this `request_id`:
                            assert!(res);
                        } else {
                            warn!("An ack from unidentified node_id: {:?}", node_id);
                        }

                        user_sender
                            .send(ServerToUserAck::Ack(request_id))
                            .await
                            .map_err(|_| ServerError::UserSenderError)?;
                    }
                    CompactToUserAck::CompactToUser(compact_to_user) => {
                        if server_state.open_nodes.contains_key(&node_id) {
                            let server_to_user = ServerToUser::Node(node_id, compact_to_user);
                            user_sender
                                .send(ServerToUserAck::ServerToUser(server_to_user))
                                .await
                                .map_err(|_| ServerError::UserSenderError)?;
                        } else {
                            // The user was told that this node is closed,
                            // so we do not want to forward any more messages from this node to the user.
                            warn!("inner_server_loop: compact_node_event from a node {:?} that is closed.", node_id);
                        }
                    }
                }
            }
            ServerEvent::NodeConnected(node_name, opt_remote_node_opened) => {
                handle_node_connected(
                    &mut server_state,
                    node_name,
                    opt_remote_node_opened,
                    &mut user_sender,
                )
                .await?;
            }
            ServerEvent::TimerTick => {
                handle_timer_tick(&mut server_state).await?;
            }
            ServerEvent::TimerClosed => return Ok(()),
        }
        // For testing:
        if let Some(ref mut event_sender) = opt_event_sender {
            let _ = event_sender.send(()).await;
        }
    }
    Ok(())
}

pub async fn compact_server_loop<ST, R, C, S>(
    conn_pair: ConnPairCompactServer,
    store: ST,
    ticks_to_connect: usize,
    timer_client: TimerClient,
    rng: R,
    connector: C,
    spawner: S,
) -> Result<(), ServerError>
where
    ST: Store,
    // TODO: Sync is probably not necessary here.
    // See https://github.com/rust-lang/rust/issues/57017
    S: Spawn + Clone + Send + Sync + 'static,
    R: CryptoRandom + Clone + 'static,
    C: FutTransform<Input = NetAddress, Output = Option<ConnPairVec>> + Clone + Send + 'static,
{
    // `opt_event_sender` is not needed in production:
    let opt_event_sender = None;
    inner_server_loop(
        conn_pair,
        store,
        ticks_to_connect,
        timer_client,
        rng,
        connector,
        spawner,
        opt_event_sender,
    )
    .await
}
