use std::collections::HashMap;

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::Duration;

use futures::executor::ThreadPool;
use futures::task::SpawnExt;

use structopt::StructOpt;

use common::conn::Listener;
use common::int_convert::usize_to_u64;

use crypto::crypto_rand::system_random;

use identity::{create_identity, IdentityClient};

use index_server::{net_index_server, NetIndexServerError};
use proto::consts::{MAX_FRAME_LENGTH, TICK_MS};
use timer::create_timer;

use net::{NetConnector, TcpListener};

use proto::file::identity::load_identity_from_file;
use proto::file::index_server::{load_trusted_servers, IndexServerDirectoryError};

// TODO; Maybe take as a command line argument in the future?
/// Maximum amount of concurrent encrypted channel set-ups.
/// We set this number to avoid DoS from half finished encrypted channel negotiations.
pub const MAX_CONCURRENT_ENCRYPT: usize = 0x200;
/// Amount of ticks we wait before attempting to reconnect to a remote index server.
pub const BACKOFF_TICKS: usize = 0x8;

/// stindex: Offst Index Server
/// A server used to index the Offst network. Collects topology information from nodes, and serves
/// nodes requests for routes
#[derive(Debug, StructOpt)]
#[structopt(name = "stindex")]
pub struct StIndexCmd {
    /// StCtrl app identity file path
    #[structopt(parse(from_os_str), short = "i", long = "idfile")]
    pub idfile: PathBuf,
    /// Listening address for clients
    #[structopt(short = "c", long = "lclient")]
    pub lclient: SocketAddr,
    /// Listening address for servers
    #[structopt(short = "s", long = "lserver")]
    pub lserver: SocketAddr,
    /// Directory path of trusted index servers
    #[structopt(parse(from_os_str), short = "t", long = "trusted")]
    pub trusted: PathBuf,
}

#[allow(clippy::enum_variant_names)]
#[derive(Debug)]
pub enum IndexServerBinError {
    CreateThreadPoolError,
    CreateTimerError,
    NetIndexServerError(NetIndexServerError),
    LoadIdentityError,
    CreateIdentityError,
    LoadTrustedServersError(IndexServerDirectoryError),
}

pub fn stindex(st_index_cmd: StIndexCmd) -> Result<(), IndexServerBinError> {
    let StIndexCmd {
        idfile,
        lclient,
        lserver,
        trusted,
    } = st_index_cmd;

    let identity = load_identity_from_file(Path::new(&idfile))
        .map_err(|_| IndexServerBinError::LoadIdentityError)?;

    let trusted_servers = load_trusted_servers(Path::new(&trusted))
        .map_err(IndexServerBinError::LoadTrustedServersError)?
        .into_iter()
        .map(|index_server_address| {
            (
                index_server_address.public_key,
                index_server_address.address,
            )
        })
        .collect::<HashMap<_, _>>();

    // Create a ThreadPool:
    let mut thread_pool =
        ThreadPool::new().map_err(|_| IndexServerBinError::CreateThreadPoolError)?;

    // A thread pool for blocking computations:
    let resolve_thread_pool =
        ThreadPool::new().map_err(|_| IndexServerBinError::CreateThreadPoolError)?;

    // A thread pool for graph computations:
    let graph_service_thread_pool =
        ThreadPool::new().map_err(|_| IndexServerBinError::CreateThreadPoolError)?;

    // Spawn identity service:
    let (sender, identity_loop) = create_identity(identity);
    thread_pool
        .spawn(identity_loop)
        .map_err(|_| IndexServerBinError::CreateIdentityError)?;
    let identity_client = IdentityClient::new(sender);

    // Get a timer client:
    let dur = Duration::from_millis(usize_to_u64(TICK_MS).unwrap());
    let timer_client = create_timer(dur, thread_pool.clone())
        .map_err(|_| IndexServerBinError::CreateTimerError)?;

    // Start listening to clients:
    let client_tcp_listener = TcpListener::new(MAX_FRAME_LENGTH, thread_pool.clone());
    let (_config_sender, incoming_client_raw_conns) = client_tcp_listener.listen(lclient);

    // Start listening to servers:
    let server_tcp_listener = TcpListener::new(MAX_FRAME_LENGTH, thread_pool.clone());
    let (_config_sender, incoming_server_raw_conns) = server_tcp_listener.listen(lserver);

    // A tcp connector, Used to connect to remote servers:
    let raw_server_net_connector =
        NetConnector::new(MAX_FRAME_LENGTH, resolve_thread_pool, thread_pool.clone());

    let rng = system_random();

    let index_server_fut = net_index_server(
        incoming_client_raw_conns,
        incoming_server_raw_conns,
        raw_server_net_connector,
        identity_client,
        timer_client,
        rng,
        trusted_servers,
        MAX_CONCURRENT_ENCRYPT,
        BACKOFF_TICKS,
        graph_service_thread_pool,
        thread_pool.clone(),
    );

    thread_pool
        .run(index_server_fut)
        .map_err(IndexServerBinError::NetIndexServerError)?;

    Ok(())
}
