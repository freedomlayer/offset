use std::fmt::Debug;
use std::fs;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

use derive_more::From;

use futures::channel::mpsc;
use futures::executor::{block_on, ThreadPool};
use futures::task::SpawnExt;
use futures::{FutureExt, TryFutureExt};

use structopt::StructOpt;

use common::conn::Listener;
use common::int_convert::usize_to_u64;

use crypto::identity::SoftwareEd25519Identity;
use crypto::rand::system_random;

use identity::{create_identity, IdentityClient};
use timer::create_timer;

use database::file_db::FileDb;
use database::{database_loop, AtomicDb, DatabaseClient};

use net::{TcpConnector, TcpListener};
use proto::consts::{
    KEEPALIVE_TICKS, MAX_FRAME_LENGTH, MAX_NODE_RELAYS, MAX_OPERATIONS_IN_BATCH, TICKS_TO_REKEY,
    TICK_MS,
};
use proto::net::messages::NetAddress;
use proto::ser_string::{deserialize_from_string, StringSerdeError};

use proto::file::IdentityFile;

use node::{NodeConfig, NodeState};

use crate::stnode::file_trusted_apps::FileTrustedApps;
use crate::stnode::net_node::{net_node, NetNodeError};

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
/*
/// Maximum amount of concurrent applications
/// going through the incoming connection transform at the same time
const MAX_CONCURRENT_INCOMING_APPS: usize = 0x8;
*/

#[allow(clippy::enum_variant_names)]
#[derive(Debug, From)]
pub enum NodeBinError {
    LoadIdentityError,
    CreateThreadPoolError,
    CreateTimerError,
    LoadDbError,
    SpawnError,
    NetNodeError(NetNodeError),
    // SerializeError(SerializeError),
    StringSerdeError(StringSerdeError),
    IoError(std::io::Error),
}

/// stnode: Offst Node
/// The decentralized credit payment engine
///
///『將欲奪之，必固與之』
///
#[derive(Debug, StructOpt)]
#[structopt(name = "stnode")]
pub struct StNodeCmd {
    /// StCtrl app identity file path
    #[structopt(parse(from_os_str), short = "i", long = "idfile")]
    pub idfile: PathBuf,
    /// Listening address (Used for communication with apps)
    #[structopt(short = "l", long = "laddr")]
    pub laddr: SocketAddr,
    /// Database file path
    #[structopt(parse(from_os_str), short = "d", long = "database")]
    pub database: PathBuf,
    /// Directory path of trusted applications
    #[structopt(parse(from_os_str), short = "t", long = "trusted")]
    pub trusted: PathBuf,
}

pub fn stnode(st_node_cmd: StNodeCmd) -> Result<(), NodeBinError> {
    let StNodeCmd {
        idfile,
        laddr,
        database,
        trusted,
    } = st_node_cmd;

    // Parse identity file:
    let identity_file: IdentityFile = deserialize_from_string(&fs::read_to_string(&idfile)?)?;
    let identity = SoftwareEd25519Identity::from_private_key(&identity_file.private_key)
        .map_err(|_| NodeBinError::LoadIdentityError)?;

    // Create a ThreadPool:
    let thread_pool = ThreadPool::new().map_err(|_| NodeBinError::CreateThreadPoolError)?;

    // Create thread pool for file system operations:
    let file_system_thread_pool =
        ThreadPool::new().map_err(|_| NodeBinError::CreateThreadPoolError)?;

    // Spawn identity service:
    let (sender, identity_loop) = create_identity(identity);
    thread_pool
        .spawn(identity_loop)
        .map_err(|_| NodeBinError::SpawnError)?;
    let identity_client = IdentityClient::new(sender);

    // Get a timer client:
    let dur = Duration::from_millis(usize_to_u64(TICK_MS).unwrap());
    let timer_client =
        create_timer(dur, thread_pool.clone()).map_err(|_| NodeBinError::CreateTimerError)?;

    // Fill in node configuration:
    let node_config = NodeConfig {
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
        // max_concurrent_incoming_apps: MAX_CONCURRENT_INCOMING_APPS,
         */
    };

    // A tcp connector, Used to connect to remote servers:
    let tcp_connector = TcpConnector::new(MAX_FRAME_LENGTH, thread_pool.clone());

    // Obtain secure cryptographic random:
    let rng = system_random();

    // Load database:
    let atomic_db =
        FileDb::<NodeState<NetAddress>>::load(database).map_err(|_| NodeBinError::LoadDbError)?;

    // Start listening to apps:
    let app_tcp_listener = TcpListener::new(MAX_FRAME_LENGTH, thread_pool.clone());
    let (_config_sender, incoming_app_raw_conns) = app_tcp_listener.listen(laddr);

    let trusted_apps = FileTrustedApps::new(trusted.into());

    // Get initial node_state:
    let node_state = atomic_db.get_state().clone();

    // Spawn database service:
    let (db_request_sender, incoming_db_requests) = mpsc::channel(0);
    let loop_fut = database_loop(atomic_db, incoming_db_requests, file_system_thread_pool)
        .map_err(|e| error!("database_loop() error: {:?}", e))
        .map(|_| ());

    thread_pool
        .spawn(loop_fut)
        .map_err(|_| NetNodeError::SpawnError)?;

    // Obtain a client to the database service:
    let database_client = DatabaseClient::new(db_request_sender);

    let node_fut = net_node(
        incoming_app_raw_conns,
        tcp_connector,
        timer_client,
        identity_client,
        rng,
        node_config,
        trusted_apps,
        node_state,
        database_client,
        thread_pool.clone(),
    );

    block_on(node_fut).map_err(NodeBinError::NetNodeError)
}
