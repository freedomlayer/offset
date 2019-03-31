#![feature(futures_api, async_await, await_macro, arbitrary_self_types)]
#![feature(nll)]
#![feature(generators)]
#![feature(never_type)]
#![deny(trivial_numeric_casts, warnings)]
#![allow(intra_doc_link_resolution_failure)]

#[macro_use]
extern crate log;

use std::collections::HashMap;

use std::net::SocketAddr;
use std::path::Path;
use std::time::Duration;

use futures::executor::ThreadPool;
use futures::task::SpawnExt;

use clap::{App, Arg};

use common::conn::Listener;
use common::int_convert::usize_to_u64;

use crypto::crypto_rand::system_random;

use identity::{create_identity, IdentityClient};
use timer::create_timer;

use node::{net_node, NetNodeError, NodeConfig, NodeState};

use database::file_db::FileDb;

use net::{NetConnector, TcpListener};
use proto::consts::{
    KEEPALIVE_TICKS, MAX_FRAME_LENGTH, MAX_NODE_RELAYS, MAX_OPERATIONS_IN_BATCH, TICKS_TO_REKEY,
    TICK_MS,
};
use proto::net::messages::NetAddress;

use proto::file::app::load_trusted_apps;
use proto::file::identity::load_identity_from_file;

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

#[derive(Debug)]
enum NodeBinError {
    ParseListenAddressError,
    LoadIdentityError,
    CreateThreadPoolError,
    CreateTimerError,
    LoadDbError,
    SpawnError,
    NetNodeError(NetNodeError),
}

fn run() -> Result<(), NodeBinError> {
    env_logger::init();
    let matches = App::new("Offst Node")
        .version("0.1.0")
        .author("real <real@freedomlayer.org>")
        .about("Spawns Offst Node")
        .arg(
            Arg::with_name("database")
                .short("d")
                .long("database")
                .value_name("database")
                .help("Database file path")
                .required(true),
        )
        .arg(
            Arg::with_name("idfile")
                .short("i")
                .long("idfile")
                .value_name("idfile")
                .help("Identity file path")
                .required(true),
        )
        .arg(
            Arg::with_name("laddr")
                .short("l")
                .long("laddr")
                .value_name("laddr")
                .help(
                    "Listening address (Used for communication with apps)\n\
                     Examples:\n\
                     - 0.0.0.0:1337\n\
                     - fe80::14c2:3048:b1ac:85fb:1337",
                )
                .required(true),
        )
        .arg(
            Arg::with_name("trusted")
                .short("t")
                .long("trusted")
                .value_name("trusted")
                .help("Directory path of trusted applications")
                .required(true),
        )
        .get_matches();

    // Parse listening address
    let listen_address_str = matches.value_of("laddr").unwrap();
    let listen_socket_addr: SocketAddr = listen_address_str
        .parse()
        .map_err(|_| NodeBinError::ParseListenAddressError)?;

    // Parse identity file:
    let idfile_path = matches.value_of("idfile").unwrap();
    let identity = load_identity_from_file(Path::new(&idfile_path))
        .map_err(|_| NodeBinError::LoadIdentityError)?;
    // let local_public_key = identity.get_public_key();

    // Create a ThreadPool:
    let mut thread_pool = ThreadPool::new().map_err(|_| NodeBinError::CreateThreadPoolError)?;

    // Create thread pool for file system operations:
    let file_system_thread_pool =
        ThreadPool::new().map_err(|_| NodeBinError::CreateThreadPoolError)?;

    // A thread pool for resolving network addresses:
    let resolve_thread_pool = ThreadPool::new().map_err(|_| NodeBinError::CreateThreadPoolError)?;

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
        /// Maximum amount of incoming app connections we set up at the same time
        max_concurrent_incoming_apps: MAX_CONCURRENT_INCOMING_APPS,
    };

    // A tcp connector, Used to connect to remote servers:
    let net_connector =
        NetConnector::new(MAX_FRAME_LENGTH, resolve_thread_pool, thread_pool.clone());

    // Obtain secure cryptographic random:
    let rng = system_random();

    // Load database:
    let db_path = matches.value_of("database").unwrap();
    let atomic_db = FileDb::<NodeState<NetAddress>>::load(Path::new(&db_path).to_path_buf())
        .map_err(|_| NodeBinError::LoadDbError)?;

    // Start listening to apps:
    let app_tcp_listener = TcpListener::new(MAX_FRAME_LENGTH, thread_pool.clone());
    let (_config_sender, incoming_app_raw_conns) = app_tcp_listener.listen(listen_socket_addr);

    // Create a closure for loading trusted apps map:
    let trusted_dir_path = Path::new(matches.value_of("trusted").unwrap()).to_path_buf();
    // TODO: We need a more detailed error here.
    // It might be hard for the user to detect in which file there was a problem
    // in case of an error.
    let get_trusted_apps = move || -> Option<_> {
        Some(
            load_trusted_apps(&trusted_dir_path)
                .ok()?
                .into_iter()
                .map(|trusted_app| (trusted_app.public_key, trusted_app.permissions))
                .collect::<HashMap<_, _>>(),
        )
    };

    let node_fut = net_node(
        incoming_app_raw_conns,
        net_connector,
        timer_client,
        identity_client,
        rng,
        node_config,
        get_trusted_apps,
        atomic_db,
        file_system_thread_pool.clone(),
        file_system_thread_pool.clone(),
        thread_pool.clone(),
    );

    thread_pool
        .run(node_fut)
        .map_err(|e| NodeBinError::NetNodeError(e))
}

fn main() {
    if let Err(e) = run() {
        error!("run() error: {:?}", e);
    }
}
