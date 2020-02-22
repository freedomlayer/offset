use std::collections::HashMap;
use std::path::PathBuf;

use futures::channel::mpsc;
use futures::future::RemoteHandle;
use futures::task::{Spawn, SpawnExt};
use futures::{future, FutureExt, SinkExt, TryFutureExt};

use crypto::identity::{Identity, SoftwareEd25519Identity};

use crypto::rand::{CryptoRandom, RandGen};
use crypto::test_utils::DummyRandom;

use common::test_executor::TestExecutor;

use common::conn::{BoxFuture, ConnPair};

use proto::crypto::{PrivateKey, PublicKey};

use proto::app_server::messages::{AppPermissions, NamedRelayAddress, RelayAddress};
use proto::consts::{KEEPALIVE_TICKS, MAX_NODE_RELAYS, MAX_OPERATIONS_IN_BATCH, TICKS_TO_REKEY};
use proto::index_server::messages::NamedIndexServerAddress;
use proto::net::messages::NetAddress;

use identity::{create_identity, IdentityClient};

use app::conn::AppConnTuple;
use app_client::app_connect_to_node;
use connection::create_secure_connector;

use node::{NodeConfig, NodeState};

use database::file_db::FileDb;
use database::{database_loop, AtomicDb, DatabaseClient};

use bin::stindex::net_index_server;
use bin::stnode::{net_node, TrustedApps};
use bin::strelay::net_relay_server;

use stcompact::compact_node::messages::{CompactReport, CompactToUserAck, UserToCompactAck};
use stcompact::compact_node::{compact_node, create_compact_report, CompactState, ConnPairCompact};
use stcompact::GenCryptoRandom;

use stcompact::messages::{ServerToUserAck, UserToServerAck};
use stcompact::server_loop::compact_server_loop;
use stcompact::store::open_file_store;

use timer::TimerClient;

use crate::sim_network::{net_address, SimNetworkClient};

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

fn gen_identity<R>(rng: &R) -> impl Identity
where
    R: CryptoRandom,
{
    let pkcs8 = PrivateKey::rand_gen(rng);
    SoftwareEd25519Identity::from_private_key(&pkcs8).unwrap()
}

fn create_identity_client<I, S>(identity: I, spawner: S) -> IdentityClient
where
    S: Spawn,
    I: Identity + Send + 'static,
{
    let (requests_sender, identity_server) = create_identity(identity);
    let identity_client = IdentityClient::new(requests_sender);
    spawner
        .spawn(identity_server.then(|_| future::ready(())))
        .unwrap();
    identity_client
}

pub fn app_private_key(index: u8) -> PrivateKey {
    let rng = DummyRandom::new(&[0x13, 0x36, index]);
    PrivateKey::rand_gen(&rng)
}

fn get_app_identity(index: u8) -> impl Identity {
    SoftwareEd25519Identity::from_private_key(&app_private_key(index)).unwrap()
}

fn get_node_identity(index: u8) -> impl Identity {
    let rng = DummyRandom::new(&[0x13, 0x37, index]);
    gen_identity(&rng)
}

fn get_index_server_identity(index: u8) -> impl Identity {
    let rng = DummyRandom::new(&[0x13, 0x38, index]);
    gen_identity(&rng)
}

fn get_relay_identity(index: u8) -> impl Identity {
    let rng = DummyRandom::new(&[0x13, 0x39, index]);
    gen_identity(&rng)
}

fn default_node_config() -> NodeConfig {
    NodeConfig {
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
        /*
        /// Maximum amount of incoming app connections we set up at the same time
        max_concurrent_incoming_apps: MAX_CONCURRENT_INCOMING_APPS,
        */
    }
}

#[derive(Clone)]
pub struct SimDb {
    temp_dir_path: PathBuf,
}

#[derive(Debug)]
pub struct SimDbError;

impl SimDb {
    pub fn new(temp_dir_path: PathBuf) -> Self {
        SimDb { temp_dir_path }
    }

    /// Create an empty node database
    pub fn init_node_db(&self, index: u8) -> Result<FileDb<NodeState<NetAddress>>, SimDbError> {
        let identity = get_node_identity(index);
        let local_public_key = identity.get_public_key();

        // Create a new database file:
        let db_path_buf = self.temp_dir_path.join(format!("node_db_{}", index));
        let initial_state = NodeState::<NetAddress>::new(local_public_key);
        FileDb::create(db_path_buf, initial_state).map_err(|_| SimDbError)
    }

    /// Load a database. The database should already exist,
    /// otherwise a panic happens.
    pub fn load_node_db(&self, index: u8) -> Result<FileDb<NodeState<NetAddress>>, SimDbError> {
        let db_path_buf = self.temp_dir_path.join(format!("node_db_{}", index));

        // Load database from file:
        FileDb::<NodeState<NetAddress>>::load(db_path_buf).map_err(|_| SimDbError)
    }

    /// Create an empty compact database
    pub fn init_compact_db(&self, index: u8) -> Result<FileDb<CompactState>, SimDbError> {
        // Create a new database file:
        let db_path_buf = self.temp_dir_path.join(format!("compact_db_{}", index));
        let initial_state = CompactState::new();
        FileDb::create(db_path_buf, initial_state).map_err(|_| SimDbError)
    }

    /// Load a database. The database should already exist,
    /// otherwise a panic happens.
    pub fn load_compact_db(&self, index: u8) -> Result<FileDb<CompactState>, SimDbError> {
        let db_path_buf = self.temp_dir_path.join(format!("compact_db_{}", index));

        // Load database from file:
        FileDb::<CompactState>::load(db_path_buf).map_err(|_| SimDbError)
    }

    /// Obtain a path to a store
    pub fn store_path(&self, index: u8) -> Result<PathBuf, SimDbError> {
        Ok(self.temp_dir_path.join(format!("store_{}", index)))
    }
}

pub fn listen_node_address(index: u8) -> NetAddress {
    net_address(&format!("node_{}", index))
}

fn listen_index_server_client_address(index: u8) -> NetAddress {
    net_address(&format!("index_server_client_{}", index))
}

fn listen_index_server_server_address(index: u8) -> NetAddress {
    net_address(&format!("index_server_server_{}", index))
}

fn listen_relay_address(index: u8) -> NetAddress {
    net_address(&format!("relay_{}", index))
}

pub fn named_relay_address(index: u8) -> NamedRelayAddress {
    NamedRelayAddress {
        public_key: get_relay_identity(index).get_public_key(),
        address: listen_relay_address(index),
        name: format!("named_relay_{}", index),
    }
}

pub fn relay_address(index: u8) -> RelayAddress {
    RelayAddress {
        public_key: get_relay_identity(index).get_public_key(),
        address: listen_relay_address(index),
    }
}

pub fn named_index_server_address(index: u8) -> NamedIndexServerAddress {
    NamedIndexServerAddress {
        public_key: get_index_server_identity(index).get_public_key(),
        address: listen_index_server_client_address(index),
        name: format!("named_index_server_{}", index),
    }
}

pub fn node_public_key(index: u8) -> PublicKey {
    get_node_identity(index).get_public_key()
}

pub fn relay_public_key(index: u8) -> PublicKey {
    get_relay_identity(index).get_public_key()
}

pub async fn create_app<S>(
    app_index: u8,
    sim_network_client: SimNetworkClient,
    timer_client: TimerClient,
    node_index: u8,
    spawner: S,
) -> Option<AppConnTuple>
where
    S: Spawn + Clone + Sync + Send + 'static,
{
    let identity = get_app_identity(app_index);
    let app_identity_client = create_identity_client(identity, spawner.clone());

    let node_public_key = get_node_identity(node_index).get_public_key();

    let rng = DummyRandom::new(&[0xff, 0x13, 0x36, app_index]);
    let secure_connector = create_secure_connector(
        sim_network_client,
        timer_client,
        app_identity_client,
        rng,
        spawner.clone(),
    );

    app_connect_to_node(
        secure_connector,
        node_public_key,
        listen_node_address(node_index),
        spawner.clone(),
    )
    .await
    .ok()
}

pub async fn create_compact_node<S>(
    app_index: u8,
    sim_db: SimDb,
    sim_network_client: SimNetworkClient,
    timer_client: TimerClient,
    node_index: u8,
    spawner: S,
) -> Option<(ConnPair<UserToCompactAck, CompactToUserAck>, CompactReport)>
where
    S: Spawn + Clone + Sync + Send + 'static,
{
    let app_conn_tuple = create_app(
        app_index,
        sim_network_client,
        timer_client,
        node_index,
        spawner.clone(),
    )
    .await?;

    let compact_state = CompactState::new();

    // Get a copy of `node_report`, and turn it into `compact_report`:
    let (app_permissions, node_report, conn_pair_app) = app_conn_tuple;
    let compact_report: CompactReport =
        create_compact_report(compact_state.clone(), node_report.clone());
    let app_conn_tuple = (app_permissions, node_report, conn_pair_app);

    let compact_db = sim_db
        .load_compact_db(app_index)
        .unwrap_or(sim_db.init_compact_db(app_index).unwrap());

    // Spawn database service:
    let (db_request_sender, incoming_db_requests) = mpsc::channel(0);
    let loop_fut = database_loop(compact_db, incoming_db_requests, spawner.clone())
        .map_err(|e| error!("database_loop() error: {:?}", e))
        .map(|_| ());

    spawner.spawn(loop_fut).unwrap();

    // Obtain a client to the database service:
    let database_client = DatabaseClient::new(db_request_sender);

    let (user_sender, compact_receiver) = mpsc::channel(1);
    let (compact_sender, user_receiver) = mpsc::channel(1);

    let conn_pair_compact = ConnPairCompact::from_raw(compact_sender, compact_receiver);

    let compact_gen = GenCryptoRandom(DummyRandom::new(&[0xff, 0x13, 0x3a, app_index]));
    let compact_fut = compact_node(
        app_conn_tuple,
        conn_pair_compact,
        compact_state,
        database_client,
        compact_gen,
    );
    spawner
        .spawn(compact_fut.map(|e| error!("compact_node() error: {:?}", e)))
        .unwrap();
    Some((
        ConnPair::from_raw(user_sender, user_receiver),
        compact_report,
    ))
}

pub async fn create_compact_server<S>(
    store_index: u8,
    sim_db: SimDb,
    sim_network_client: SimNetworkClient,
    timer_client: TimerClient,
    spawner: S,
) -> Option<ConnPair<UserToServerAck, ServerToUserAck>>
where
    S: Spawn + Clone + Sync + Send + 'static,
{
    let store_path_buf = sim_db.store_path(store_index).unwrap();

    let rng = DummyRandom::new(&[0xff, 0x13, 0x3b, store_index]);

    let file_store = open_file_store(store_path_buf, spawner.clone(), spawner.clone())
        .await
        .unwrap();

    // We use a bigger than 0 length for the channels here to leave room
    // for not collecting incoming messages immediately as we pass time forward.
    let (server_sender, user_receiver) = mpsc::channel(0x40);
    let (user_sender, server_receiver) = mpsc::channel(0x40);

    let ticks_to_connect: usize = 8;

    let compact_fut = compact_server_loop(
        ConnPair::from_raw(server_sender, server_receiver),
        file_store,
        ticks_to_connect,
        timer_client,
        rng,
        sim_network_client,
        spawner.clone(),
    );

    let user_conn_pair = ConnPair::from_raw(user_sender, user_receiver);

    spawner
        .spawn(compact_fut.map(|e| error!("compact_server_loop() error: {:?}", e)))
        .unwrap();

    Some(user_conn_pair)
}

#[derive(Debug, Clone)]
struct DummyTrustedApps {
    trusted_apps: HashMap<PublicKey, AppPermissions>,
}

impl TrustedApps for DummyTrustedApps {
    fn app_permissions<'a>(
        &'a mut self,
        app_public_key: &'a PublicKey,
    ) -> BoxFuture<'a, Option<AppPermissions>> {
        Box::pin(async move { self.trusted_apps.get(app_public_key).cloned() })
    }
}

pub async fn create_node<S>(
    index: u8,
    sim_db: SimDb,
    timer_client: TimerClient,
    mut sim_network_client: SimNetworkClient,
    trusted_apps: HashMap<u8, AppPermissions>,
    spawner: S,
) -> RemoteHandle<()>
where
    S: Spawn + Send + Sync + Clone + 'static,
{
    let identity = get_node_identity(index);
    let identity_client = create_identity_client(identity, spawner.clone());
    let listen_address = listen_node_address(index);
    let incoming_app_raw_conns = sim_network_client.listen(listen_address).await.unwrap();

    // Translate application index to application public key:
    let trusted_apps_map = trusted_apps
        .into_iter()
        .map(|(index, app_permissions)| (get_app_identity(index).get_public_key(), app_permissions))
        .collect::<HashMap<_, _>>();
    let dummy_trusted_apps = DummyTrustedApps {
        trusted_apps: trusted_apps_map,
    };
    // let get_trusted_apps = move || Some(trusted_apps.clone());

    let rng = DummyRandom::new(&[0xff, 0x13, 0x37, index]);

    let atomic_db = sim_db.load_node_db(index).unwrap();

    // Get initial node_state:
    let node_state = atomic_db.get_state().clone();

    // Spawn database service:
    let (db_request_sender, incoming_db_requests) = mpsc::channel(0);
    let loop_fut = database_loop(atomic_db, incoming_db_requests, spawner.clone())
        .map_err(|e| error!("database_loop() error: {:?}", e))
        .map(|_| ());

    spawner.spawn(loop_fut).unwrap();

    // Obtain a client to the database service:
    let database_client = DatabaseClient::new(db_request_sender);

    // Note: we use the same spawner for testing purposes.
    // Simulating the passage of time becomes more difficult if our code uses a few different executors.
    let net_node_fut = net_node(
        incoming_app_raw_conns,
        sim_network_client,
        timer_client,
        identity_client,
        rng,
        default_node_config(),
        dummy_trusted_apps,
        node_state,
        database_client,
        spawner.clone(),
    )
    .map_err(|e| error!("net_node() error: {:?}", e))
    .map(|_| ());

    spawner.spawn_with_handle(net_node_fut).unwrap()
}

pub async fn create_index_server<S>(
    index: u8,
    timer_client: TimerClient,
    mut sim_network_client: SimNetworkClient,
    trusted_servers: Vec<u8>,
    spawner: S,
) where
    S: Spawn + Send + Sync + Clone + 'static,
{
    let identity = get_index_server_identity(index);
    let identity_client = create_identity_client(identity, spawner.clone());
    let client_listen_address = listen_index_server_client_address(index);
    let server_listen_address = listen_index_server_server_address(index);

    let incoming_client_raw_conns = sim_network_client
        .listen(client_listen_address)
        .await
        .unwrap();
    let incoming_server_raw_conns = sim_network_client
        .listen(server_listen_address)
        .await
        .unwrap();

    // Translate index server index into a map of public_key -> NetAddress
    let trusted_servers = trusted_servers
        .into_iter()
        .map(|index| {
            (
                get_index_server_identity(index).get_public_key(),
                listen_index_server_server_address(index),
            )
        })
        .collect::<HashMap<_, _>>();

    let rng = DummyRandom::new(&[0xff, 0x13, 0x38, index]);
    // We use the same spawner for both required spawners.
    // We do this to make it easier to simulate the passage of time in tests.
    let net_index_server_fut = net_index_server(
        incoming_client_raw_conns,
        incoming_server_raw_conns,
        sim_network_client,
        identity_client,
        timer_client,
        rng,
        trusted_servers,
        MAX_CONCURRENT_ENCRYPT,
        BACKOFF_TICKS,
        spawner.clone(),
        spawner.clone(),
    )
    .map_err(|e| error!("net_index_server()  error: {:?}", e))
    .map(|_| ());

    spawner.spawn(net_index_server_fut).unwrap();
}

pub async fn create_relay<S>(
    index: u8,
    timer_client: TimerClient,
    mut sim_network_client: SimNetworkClient,
    spawner: S,
) where
    S: Spawn + Send + Sync + Clone + 'static,
{
    let identity = get_relay_identity(index);
    let identity_client = create_identity_client(identity, spawner.clone());

    let listen_address = listen_relay_address(index);
    let incoming_raw_conns = sim_network_client.listen(listen_address).await.unwrap();

    let rng = DummyRandom::new(&[0xff, 0x13, 0x39, index]);
    let net_relay_server_fut = net_relay_server(
        incoming_raw_conns,
        identity_client,
        timer_client,
        rng,
        MAX_CONCURRENT_ENCRYPT,
        spawner.clone(),
    )
    .map_err(|e| error!("net_relay_server() error: {:?}", e))
    .map(|_| ());

    spawner.spawn(net_relay_server_fut).unwrap();
}

pub async fn advance_time<'a>(
    ticks: usize,
    tick_sender: &'a mut mpsc::Sender<()>,
    test_executor: &'a TestExecutor,
) {
    test_executor.wait().await;
    for _ in 0..ticks {
        tick_sender.send(()).await.unwrap();
        test_executor.wait().await;
    }
}
