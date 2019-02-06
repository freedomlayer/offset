#![feature(futures_api, async_await, await_macro, arbitrary_self_types)]
#![feature(nll)]
#![feature(try_from)]
#![feature(generators)]
#![feature(never_type)]

#[macro_use]
extern crate log;

use std::path::Path;
use std::time::Duration;

use futures::executor::ThreadPool;
use futures::task::SpawnExt;
use futures::channel::mpsc;
use futures::{FutureExt, TryFutureExt};

use clap::{Arg, App};
use log::Level;

use common::int_convert::usize_to_u64;
use common::conn::{FutTransform, FuncFutTransform};
use crypto::identity::{Identity, PublicKey};
use identity::{create_identity, IdentityClient};
use timer::create_timer;
use version::VersionPrefix;
use crypto::crypto_rand::system_random;

use node::{node, NodeConfig, NodeState, NodeError};

use database::file_db::FileDb;
use database::{database_loop, DatabaseClient, AtomicDb};

use proto::consts::{PROTOCOL_VERSION, KEEPALIVE_TICKS, TICKS_TO_REKEY, 
    MAX_OPERATIONS_IN_BATCH, TICK_MS, MAX_FRAME_LENGTH};
use proto::index_server::messages::IndexServerAddress;
use proto::funder::messages::RelayAddress;
use net::TcpConnector;

use bin::load_identity_from_file;

/// Memory allocated to a channel in memory (Used to connect two components)
const CHANNEL_LEN: usize = 0x20;
/// The amount of ticks we wait before attempting to reconnect
const BACKOFF_TICKS: usize = 0x8;
/// Maximum amount of encryption set ups (diffie hellman) that we allow to occur at the same
/// time.
const MAX_CONCURREN_ENCRYPT: usize = 0x8;
/// The size we allocate for the user send funds requests queue.
const MAX_PENDING_USER_REQUESTS: usize = 0x20;
/// Maximum amount of concurrent index client requests:
const MAX_OPEN_INDEX_CLIENT_REQUESTS: usize = 0x8;
/// The amount of ticks we are willing to wait until a connection is established (Through
/// the relay)
const CONN_TIMEOUT_TICKS: usize = 0x8;


#[derive(Debug)]
enum NodeBinError {
    LoadIdentityError,
    CreateThreadPoolError,
    CreateIdentityError,
    CreateTimerError,
    LoadDbError,
    SpawnError,
    DatabaseIdentityMismatch,
    NodeError(NodeError),
}

fn run() -> Result<(), NodeBinError> {
    simple_logger::init_with_level(Level::Warn).unwrap();
    let matches = App::new("Offst Node")
                          .version("0.1")
                          .author("real <real@freedomlayer.org>")
                          .about("Spawns Offst Node")
                          .arg(Arg::with_name("database")
                               .short("d")
                               .long("database")
                               .value_name("database")
                               .help("Database file path")
                               .required(true))
                          .arg(Arg::with_name("idfile")
                               .short("i")
                               .long("idfile")
                               .value_name("idfile")
                               .help("Identity file path")
                               .required(true))
                          .get_matches();

    // Parse identity file:
    let idfile_path = matches.value_of("idfile").unwrap();
    let identity = load_identity_from_file(Path::new(&idfile_path))
        .map_err(|_| NodeBinError::LoadIdentityError)?;
    let local_public_key = identity.get_public_key();

    // Create a ThreadPool:
    let mut thread_pool = ThreadPool::new()
        .map_err(|_| NodeBinError::CreateThreadPoolError)?;

    // Spawn identity service:
    let (sender, identity_loop) = create_identity(identity);
    thread_pool.spawn(identity_loop)
        .map_err(|_| NodeBinError::CreateIdentityError)?;
    let identity_client = IdentityClient::new(sender);

    // Get a timer client:
    let dur = Duration::from_millis(usize_to_u64(TICK_MS).unwrap()); 
    let mut timer_client = create_timer(dur, thread_pool.clone())
        .map_err(|_| NodeBinError::CreateTimerError)?;

    // Fill in node configuration:
    let node_config =  NodeConfig {
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
        max_concurrent_encrypt: MAX_CONCURREN_ENCRYPT,
        /// The amount of ticks we are willing to wait until a connection is established (Through
        /// the relay)
        conn_timeout_ticks: CONN_TIMEOUT_TICKS,
        /// Maximum amount of operations in one move token message
        max_operations_in_batch: MAX_OPERATIONS_IN_BATCH,
        /// The size we allocate for the user send funds requests queue.
        max_pending_user_requests: MAX_PENDING_USER_REQUESTS,
        /// Maximum amount of concurrent index client requests:
        max_open_index_client_requests: MAX_OPEN_INDEX_CLIENT_REQUESTS,
    };

    // A tcp connector, Used to connect to remote servers:
    let net_connector = TcpConnector::new(MAX_FRAME_LENGTH, thread_pool.clone());

    // Wrap net connector with a version prefix:
    let version_transform = VersionPrefix::new(PROTOCOL_VERSION, 
                                                   thread_pool.clone());
    let version_connector = FuncFutTransform::new(move |address| {
        let mut c_net_connector = net_connector.clone();
        let mut c_version_transform = version_transform.clone();
        Box::pin(async move {
            let conn_pair = await!(c_net_connector.transform(address))?;
            Some(await!(c_version_transform.transform(conn_pair)))
        })
    });

    // Obtain secure cryptographic random:
    let rng = system_random();

    // Load database:
    let db_path = matches.value_of("database").unwrap();
    let atomic_db = FileDb::<NodeState<RelayAddress, IndexServerAddress>>::load(Path::new(&db_path).to_path_buf())
        .map_err(|_| NodeBinError::LoadDbError)?;

    // Get initial node_state:
    let node_state = atomic_db.get_state().clone();

    // Make sure that the local public key in the database
    // matches the local public key from the provided identity file:
    if node_state.funder_state.local_public_key != local_public_key {
        return Err(NodeBinError::DatabaseIdentityMismatch);
    }

    // Spawn database service:
    let (db_request_sender, incoming_db_requests) = mpsc::channel(0);
    let loop_fut = database_loop(atomic_db, incoming_db_requests)
        .map_err(|e| error!("database_loop() error: {:?}", e))
        .map(|_| ());
    thread_pool.spawn(loop_fut)
        .map_err(|_| NodeBinError::SpawnError)?;

    // Obtain a client to the database service:
    let database_client = DatabaseClient::new(db_request_sender);

    // TODO: 
    // - TcpListener to get incoming apps
    // - Configuration directory for trusted apps? For each app:
    //      - Public key
    //      - permissions

    /*
    let node_fut = node(
        node_config,
        identity_client,
        timer_client,
        node_state,
        database_client,
        version_connector,
        incoming_apps: IA,
        rng,
        thread_pool.clone());

    thread_pool.run(node_fut)
        .map_err(|e| NodeBinError::NodeError(e))
    */
    unimplemented!();
}

fn main() {
    if let Err(e) = run() {
        error!("run() error: {:?}", e);
    }
}
