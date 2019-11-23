use std::collections::HashMap;
use std::path::PathBuf;

use futures::channel::mpsc;
use futures::future::RemoteHandle;
use futures::task::{Spawn, SpawnExt};
use futures::{future, FutureExt, SinkExt, TryFutureExt};

use crypto::identity::{generate_private_key, Identity, SoftwareEd25519Identity};

use crypto::rand::CryptoRandom;
use crypto::test_utils::DummyRandom;

use common::test_executor::TestExecutor;

use proto::crypto::PublicKey;

use proto::app_server::messages::{AppPermissions, NamedRelayAddress, RelayAddress};
use proto::consts::{KEEPALIVE_TICKS, MAX_NODE_RELAYS, MAX_OPERATIONS_IN_BATCH, TICKS_TO_REKEY};
use proto::index_server::messages::NamedIndexServerAddress;
use proto::net::messages::NetAddress;

use identity::{create_identity, IdentityClient};

use app::conn::{inner_connect, AppConnTuple};
use node::{net_node, NodeConfig, NodeState};

use database::file_db::FileDb;

use index_server::net_index_server;
use relay::net_relay_server;

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
/// Maximum amount of concurrent applications
/// going through the incoming connection transform at the same time
const MAX_CONCURRENT_INCOMING_APPS: usize = 0x8;

/*
// Based on:
// - https://rust-lang-nursery.github.io/futures-api-docs/0.3.0-alpha.13/src/futures_test/future/pending_once.rs.html#14-17
// - https://github.com/rust-lang-nursery/futures-rs/issues/869
pub struct Yield(usize);

impl Yield {
    pub fn new(num_yields: usize) -> Self {
        Yield(num_yields)
    }
}

impl Future for Yield {
    type Output = ();
    fn poll(mut self: Pin<&mut Self>, waker: &Waker) -> Poll<Self::Output> {
        let count = &mut self.as_mut().0;
        *count = count.saturating_sub(1);
        if *count == 0 {
            Poll::Ready(())
        } else {
            waker.wake();
            Poll::Pending
        }
    }
}
*/

fn gen_identity<R>(rng: &R) -> impl Identity
where
    R: CryptoRandom,
{
    let pkcs8 = generate_private_key(rng);
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

fn get_app_identity(index: u8) -> impl Identity {
    let rng = DummyRandom::new(&[0x13, 0x36, index]);
    gen_identity(&rng)
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
        /// Maximum amount of incoming app connections we set up at the same time
        max_concurrent_incoming_apps: MAX_CONCURRENT_INCOMING_APPS,
    }
}

#[derive(Clone)]
pub struct SimDb {
    temp_dir_path: PathBuf,
}

impl SimDb {
    pub fn new(temp_dir_path: PathBuf) -> Self {
        SimDb { temp_dir_path }
    }

    /// Create an empty node database
    pub fn init_db(&self, index: u8) -> FileDb<NodeState<NetAddress>> {
        let identity = get_node_identity(index);
        let local_public_key = identity.get_public_key();

        // Create a new database file:
        let db_path_buf = self.temp_dir_path.join(format!("db_{}", index));
        let initial_state = NodeState::<NetAddress>::new(local_public_key);
        FileDb::create(db_path_buf, initial_state).unwrap()
    }

    /// Load a database. The database should already exist,
    /// otherwise a panic happens.
    pub fn load_db(&self, index: u8) -> FileDb<NodeState<NetAddress>> {
        let db_path_buf = self.temp_dir_path.join(format!("db_{}", index));

        // Load database from file:
        FileDb::<NodeState<NetAddress>>::load(db_path_buf).unwrap()
    }
}

fn listen_node_address(index: u8) -> NetAddress {
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
    index: u8,
    sim_network_client: SimNetworkClient,
    timer_client: TimerClient,
    node_index: u8,
    spawner: S,
) -> Option<AppConnTuple>
where
    S: Spawn + Clone + Sync + Send + 'static,
{
    let identity = get_app_identity(index);
    let app_identity_client = create_identity_client(identity, spawner.clone());

    let node_public_key = get_node_identity(node_index).get_public_key();

    let rng = DummyRandom::new(&[0xff, 0x13, 0x36, index]);
    inner_connect(
        sim_network_client,
        node_public_key,
        listen_node_address(node_index),
        timer_client,
        app_identity_client,
        rng,
        spawner.clone(),
    )
    .await
    .ok()
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
    let trusted_apps = trusted_apps
        .into_iter()
        .map(|(index, app_permissions)| (get_app_identity(index).get_public_key(), app_permissions))
        .collect::<HashMap<_, _>>();
    let get_trusted_apps = move || Some(trusted_apps.clone());

    let rng = DummyRandom::new(&[0xff, 0x13, 0x37, index]);
    // Note: we use the same spawner for testing purposes.
    // Simulating the passage of time becomes more difficult if our code uses a few different executors.
    let net_node_fut = net_node(
        incoming_app_raw_conns,
        sim_network_client,
        timer_client,
        identity_client,
        rng,
        default_node_config(),
        get_trusted_apps,
        sim_db.load_db(index),
        spawner.clone(), // trusted_apps_spawner
        spawner.clone(), // database_spawner
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
