#![feature(futures_api, async_await, await_macro, arbitrary_self_types)]
#![feature(nll)]
#![feature(try_from)]
#![feature(generators)]
#![feature(never_type)]

#[macro_use]
extern crate log;

use std::collections::HashMap;

use std::net::SocketAddr;
use std::path::Path;
use std::time::Duration;

use futures::executor::ThreadPool;
use futures::task::{Spawn, SpawnExt};
use futures::channel::mpsc;
use futures::{future, FutureExt, TryFutureExt, StreamExt, SinkExt};

use clap::{Arg, App};
use log::Level;

use common::int_convert::usize_to_u64;
use common::conn::{FutTransform, FuncFutTransform, 
    Listener, BoxFuture, ConnPairVec};
use common::transform_pool::transform_pool_loop;

use crypto::identity::{Identity, PublicKey};
use crypto::crypto_rand::system_random;

use identity::{create_identity, IdentityClient};
use timer::create_timer;

use version::VersionPrefix;
use secure_channel::SecureChannel;
use keepalive::KeepAliveChannel;

use node::{node, NodeConfig, NodeState, 
    NodeError, IncomingAppConnection, AppPermissions};

use database::file_db::FileDb;
use database::{database_loop, DatabaseClient, AtomicDb};

use proto::consts::{PROTOCOL_VERSION, KEEPALIVE_TICKS, TICKS_TO_REKEY, 
    MAX_OPERATIONS_IN_BATCH, TICK_MS, MAX_FRAME_LENGTH};
use proto::index_server::messages::IndexServerAddress;
use proto::funder::messages::RelayAddress;
use proto::app_server::serialize::{deserialize_app_to_app_server,
                                   serialize_app_server_to_app};
use net::{TcpConnector, TcpListener, socket_addr_to_tcp_address};


use bin::{load_identity_from_file, load_trusted_apps};

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
const MAX_CONCURRENT_INCOMING_APP: usize = 0x8;


#[derive(Debug)]
enum NodeBinError {
    ParseListenAddressError,
    LoadIdentityError,
    LoadTrustedAppsError,
    CreateThreadPoolError,
    CreateIdentityError,
    CreateTimerError,
    LoadDbError,
    SpawnError,
    DatabaseIdentityMismatch,
    NodeError(NodeError),
}

#[derive(Clone)]
struct AppConnTransform<VT,ET,KT,S,GT> {
    version_transform: VT, 
    encrypt_transform: ET, 
    keepalive_transform: KT,
    spawner: S,
    get_trusted_apps: GT,
    thread_pool: ThreadPool,
}

impl<VT,ET,KT,S,GT> AppConnTransform<VT,ET,KT,S,GT> {
    pub fn new(version_transform: VT,
               encrypt_transform: ET,
               keepalive_transform: KT,
               spawner: S,
               get_trusted_apps: GT) -> Result<Self, NodeBinError> {

        // Create a extra inner thread pool.
        // We use another thread pool because we don't want to block
        // the main thread pool when we are reading files from the filesystem.
        // TODO: What is the idiomatic way to do this without having an extra ThreadPool?
        let thread_pool = ThreadPool::new()
            .map_err(|_| NodeBinError::CreateThreadPoolError)?;

        Ok(AppConnTransform {
            version_transform, 
            encrypt_transform, 
            keepalive_transform,
            spawner,
            get_trusted_apps,
            thread_pool,
        })
    }
}

impl<VT,ET,KT,S,GT> FutTransform for AppConnTransform<VT,ET,KT,S,GT> 
where
    VT: FutTransform<Input=ConnPairVec, Output=ConnPairVec> + Clone + Send,
    ET: FutTransform<Input=(Option<PublicKey>, ConnPairVec),
                     Output=Option<(PublicKey, ConnPairVec)>> + Clone + Send,
    KT: FutTransform<Input=ConnPairVec, Output=ConnPairVec> + Clone + Send,
    S: Spawn + Clone + Send,
    GT: Fn() -> Result<HashMap<PublicKey, AppPermissions>, NodeBinError> + Clone + Send + 'static,
{
    type Input = ConnPairVec;
    type Output = Option<IncomingAppConnection<RelayAddress,IndexServerAddress>>;

    fn transform(&mut self, conn_pair: Self::Input)
        -> BoxFuture<'_, Self::Output> {
        
        // TODO: 
        // - TcpListener to get incoming apps
        // - Configuration directory for trusted apps? For each app:
        //      - Public key
        //      - permissions
        //
        // - For each incoming connection:
        //      - version prefix
        //      - encrypt (Checking against trusted apps list 
        //              + Obtain permissions)
        //      - keepalive
        //      - serialization

        Box::pin(async move {
            // Version perfix:
            let ver_conn = await!(self.version_transform.transform(conn_pair));
            // Encrypt:
            let (public_key, enc_conn) = await!(self.encrypt_transform.transform((None, ver_conn)))?;

            // Obtain permissions for app (Or reject it if not trusted):
            let c_get_trusted_apps = self.get_trusted_apps.clone();

            // Obtain trusted apps using a separate thread pool:
            // At this point we re-read the directory of all trusted apps.
            // This could be slow, therefore we perform this operation on self.thread_pool
            // and not on self.spawner, which is the main thread_pool for this program.
            let trusted_apps_fut = self.thread_pool.spawn_with_handle(
                future::lazy(move |_| (c_get_trusted_apps)())).ok()?;
            let trusted_apps = await!(trusted_apps_fut).ok()?;

            let app_permissions = trusted_apps.get(&public_key)?;

            // Keepalive wrapper:
            let (mut sender, mut receiver) = await!(self.keepalive_transform.transform(enc_conn));

            // serialization:
            let (user_sender, mut from_user_sender) = mpsc::channel(0);
            let (mut to_user_receiver, user_receiver) = mpsc::channel(0);

            // Deserialize received data
            let _ = self.spawner.spawn(async move {
                while let Some(data) = await!(receiver.next()) {
                    let message = match deserialize_app_to_app_server(&data) {
                        Ok(message) => message,
                        Err(_) => return,
                    };
                    if let Err(_) = await!(to_user_receiver.send(message)) {
                        return;
                    }
                }
            });

            // Serialize sent data:
            let _ = self.spawner.spawn(async move {
                while let Some(message) = await!(from_user_sender.next()) {
                    let data = serialize_app_server_to_app(&message);
                    if let Err(_) = await!(sender.send(data)) {
                        return;
                    }
                }
            });

            Some((app_permissions.clone(), (user_sender, user_receiver)))
        })
    }
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
                          .arg(Arg::with_name("laddr")
                               .short("l")
                               .long("laddr")
                               .value_name("laddr")
                               .help("Listening address. \nExamples:\n- 0.0.0.0:1337\n- fe80::14c2:3048:b1ac:85fb:1337")
                               .required(true))
                          .arg(Arg::with_name("trusted")
                               .short("t")
                               .long("trusted")
                               .value_name("trusted")
                               .help("Directory path of trusted applications")
                               .required(true))
                          .get_matches();


    // Parse listening address
    let listen_address_str = matches.value_of("laddr").unwrap();
    let socket_addr: SocketAddr = listen_address_str.parse()
        .map_err(|_| NodeBinError::ParseListenAddressError)?;
    let listen_tcp_address = socket_addr_to_tcp_address(&socket_addr);

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
    let timer_client = create_timer(dur, thread_pool.clone())
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
    };

    // A tcp connector, Used to connect to remote servers:
    let net_connector = TcpConnector::new(MAX_FRAME_LENGTH, thread_pool.clone());

    // Wrap net connector with a version prefix:
    let version_transform = VersionPrefix::new(PROTOCOL_VERSION, 
                                                   thread_pool.clone());
    let c_version_transform = version_transform.clone();
    let version_connector = FuncFutTransform::new(move |address| {
        let mut c_net_connector = net_connector.clone();
        let mut c_version_transform = c_version_transform.clone();
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

    // Start listening to apps:
    let app_tcp_listener = TcpListener::new(MAX_FRAME_LENGTH, thread_pool.clone());
    let (_config_sender, incoming_app_raw_conns) = app_tcp_listener.listen(listen_tcp_address);

    let encrypt_transform = SecureChannel::new(
        identity_client.clone(),
        rng.clone(),
        timer_client.clone(),
        TICKS_TO_REKEY,
        thread_pool.clone());

    let keepalive_transform = KeepAliveChannel::new(
        timer_client.clone(),
        KEEPALIVE_TICKS,
        thread_pool.clone());

    // Create a closure for loading trusted apps map:
    let trusted_dir_path = Path::new(matches.value_of("trusted").unwrap()).to_path_buf();
    // TODO: We need a more detailed error here.
    // It might be hard for the user to detect in which file there was a problem
    // in case of an error.
    let get_trusted_apps = move || -> Result<_, NodeBinError> {
        Ok(load_trusted_apps(&trusted_dir_path)
        .map_err(|_| NodeBinError::LoadTrustedAppsError)?
        .into_iter()
        .map(|trusted_app| (trusted_app.public_key, trusted_app.permissions))
        .collect::<HashMap<_,_>>())
    };

    let app_conn_transform = AppConnTransform::new(
        version_transform,
        encrypt_transform,
        keepalive_transform,
        thread_pool.clone(),
        get_trusted_apps)?;

    let (incoming_apps_sender, incoming_apps) = mpsc::channel(0);

    // Apply transform over every incoming app connection:
    let pool_fut = transform_pool_loop(
            incoming_app_raw_conns,
            incoming_apps_sender,
            app_conn_transform,
            MAX_CONCURRENT_INCOMING_APP,
            thread_pool.clone())
        .map_err(|e| error!("transform_pool_loop() error: {:?}", e))
        .map(|_| ());

    thread_pool.spawn(pool_fut)
        .map_err(|_| NodeBinError::SpawnError)?;
    
    let node_fut = node(
        node_config,
        identity_client,
        timer_client,
        node_state,
        database_client,
        version_connector,
        incoming_apps,
        rng,
        thread_pool.clone());

    thread_pool.run(node_fut)
        .map_err(|e| NodeBinError::NodeError(e))
}

fn main() {
    if let Err(e) = run() {
        error!("run() error: {:?}", e);
    }
}
