use std::collections::HashMap;

use std::fs;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::Duration;

use futures::executor::{block_on, ThreadPool};
use futures::task::SpawnExt;

use structopt::StructOpt;

use common::conn::Listener;
use common::int_convert::usize_to_u64;

use crypto::identity::SoftwareEd25519Identity;
use crypto::rand::system_random;

use identity::{create_identity, IdentityClient};

use derive_more::From;

use crate::stindex::net_index::{net_index_server, NetIndexServerError};
use proto::consts::{MAX_FRAME_LENGTH, TICK_MS};
use timer::create_timer;

use net::{TcpConnector, TcpListener};

// use proto::file::identity::load_identity_from_file;
// use proto::file::index_server::{load_trusted_servers, IndexServerDirectoryError};
use proto::file::{IdentityFile, IndexServerFile};
use proto::ser_string::{deserialize_from_string, StringSerdeError};

// TODO: Maybe take as a command line argument in the future?
/// Maximum amount of concurrent encrypted channel set-ups.
/// We set this number to avoid DoS from half finished encrypted channel negotiations.
pub const MAX_CONCURRENT_ENCRYPT: usize = 0x200;
/// Amount of ticks we wait before attempting to reconnect to a remote index server.
pub const BACKOFF_TICKS: usize = 0x8;

/// stindex: Offset Index Server
/// A server used to index the Offset network. Collects topology information from nodes, and serves
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
#[derive(Debug, From)]
pub enum IndexServerBinError {
    CreateThreadPoolError,
    CreateTimerError,
    NetIndexServerError(NetIndexServerError),
    LoadIdentityError,
    CreateIdentityError,
    // LoadTrustedServersError(IndexServerDirectoryError),
    IoError(std::io::Error),
    StringSerdeError(StringSerdeError),
}

/// Load a directory of index server address files, and return a map representing
/// the information from all files
pub fn load_trusted_servers(dir_path: &Path) -> Result<Vec<IndexServerFile>, IndexServerBinError> {
    let mut res_trusted = Vec::new();
    for entry in fs::read_dir(dir_path)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            continue;
        }
        res_trusted.push(deserialize_from_string(&fs::read_to_string(&path)?)?);
    }
    Ok(res_trusted)
}

pub fn stindex(st_index_cmd: StIndexCmd) -> Result<(), IndexServerBinError> {
    let StIndexCmd {
        idfile,
        lclient,
        lserver,
        trusted,
    } = st_index_cmd;

    let identity_file: IdentityFile = deserialize_from_string(&fs::read_to_string(&idfile)?)?;
    let identity = SoftwareEd25519Identity::from_private_key(&identity_file.private_key)
        .map_err(|_| IndexServerBinError::LoadIdentityError)?;

    let trusted_servers = load_trusted_servers(Path::new(&trusted))?
        .into_iter()
        .map(|index_server_file| (index_server_file.public_key, index_server_file.address))
        .collect::<HashMap<_, _>>();

    // Create a ThreadPool:
    let thread_pool = ThreadPool::new().map_err(|_| IndexServerBinError::CreateThreadPoolError)?;

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
    let raw_server_net_connector = TcpConnector::new(MAX_FRAME_LENGTH, thread_pool.clone());

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
        thread_pool,
    );

    block_on(index_server_fut).map_err(IndexServerBinError::NetIndexServerError)?;

    Ok(())
}
