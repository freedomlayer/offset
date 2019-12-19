use std::fs;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

use derive_more::From;

use futures::executor::{block_on, ThreadPool};
use futures::task::SpawnExt;

use structopt::StructOpt;

use common::conn::Listener;

use crypto::identity::SoftwareEd25519Identity;
use crypto::rand::system_random;
use identity::{create_identity, IdentityClient};

use proto::consts::{MAX_FRAME_LENGTH, TICK_MS};

use common::int_convert::usize_to_u64;

use crate::strelay::net_relay::{net_relay_server, NetRelayServerError};
use net::TcpListener;
use timer::create_timer;

use proto::file::IdentityFile;
use proto::ser_string::{deserialize_from_string, StringSerdeError};

// TODO: Maybe take as a command line argument in the future?
/// Maximum amount of concurrent encrypted channel set-ups.
/// We set this number to avoid DoS from half finished encrypted channel negotiations.
pub const MAX_CONCURRENT_ENCRYPT: usize = 0x200;

#[allow(clippy::enum_variant_names)]
#[derive(Debug, From)]
pub enum RelayServerBinError {
    CreateThreadPoolError,
    LoadIdentityError,
    CreateIdentityError,
    CreateTimerError,
    NetRelayServerError(NetRelayServerError),
    IoError(std::io::Error),
    StringSerdeError(StringSerdeError),
}

/// strelay: Offst Relay Server
/// A server used to relay communication between nodes
#[derive(Debug, StructOpt)]
#[structopt(name = "strelay")]
pub struct StRelayCmd {
    /// StCtrl app identity file path
    #[structopt(parse(from_os_str), short = "i", long = "idfile")]
    pub idfile: PathBuf,
    /// Listening address (Example: 0.0.0.0:1337)
    #[structopt(short = "l", long = "laddr")]
    pub laddr: SocketAddr,
}

pub fn strelay(st_relay_cmd: StRelayCmd) -> Result<(), RelayServerBinError> {
    let StRelayCmd { idfile, laddr } = st_relay_cmd;

    // Parse identity file:
    let identity_file: IdentityFile = deserialize_from_string(&fs::read_to_string(&idfile)?)?;
    let identity = SoftwareEd25519Identity::from_private_key(&identity_file.private_key)
        .map_err(|_| RelayServerBinError::LoadIdentityError)?;

    // Create a ThreadPool:
    let thread_pool = ThreadPool::new().map_err(|_| RelayServerBinError::CreateThreadPoolError)?;

    // Spawn identity service:
    let (sender, identity_loop) = create_identity(identity);
    thread_pool
        .spawn(identity_loop)
        .map_err(|_| RelayServerBinError::CreateIdentityError)?;
    let identity_client = IdentityClient::new(sender);

    let dur = Duration::from_millis(usize_to_u64(TICK_MS).unwrap());
    let timer_client = create_timer(dur, thread_pool.clone())
        .map_err(|_| RelayServerBinError::CreateTimerError)?;

    let rng = system_random();

    let tcp_listener = TcpListener::new(MAX_FRAME_LENGTH, thread_pool.clone());
    let (_config_sender, incoming_raw_conns) = tcp_listener.listen(laddr);

    let relay_server_fut = net_relay_server(
        incoming_raw_conns,
        identity_client,
        timer_client,
        rng,
        MAX_CONCURRENT_ENCRYPT,
        thread_pool.clone(),
    );

    block_on(relay_server_fut).map_err(RelayServerBinError::NetRelayServerError)
}
