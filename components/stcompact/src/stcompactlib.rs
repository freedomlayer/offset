use std::collections::HashMap;

use std::fs;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::Duration;

use derive_more::From;

use futures::executor::{block_on, ThreadPool};
use futures::task::SpawnExt;

use structopt::StructOpt;

use common::conn::Listener;
use common::int_convert::usize_to_u64;

use crypto::identity::SoftwareEd25519Identity;
use crypto::rand::system_random;

use identity::{create_identity, IdentityClient};
use timer::create_timer;

use node::{net_node, NetNodeError, NodeConfig, NodeState};

use database::file_db::FileDb;

use net::{TcpConnector, TcpListener};
use proto::consts::{
    KEEPALIVE_TICKS, MAX_FRAME_LENGTH, MAX_NODE_RELAYS, MAX_OPERATIONS_IN_BATCH, TICKS_TO_REKEY,
    TICK_MS,
};
use proto::net::messages::NetAddress;
use proto::ser_string::{deserialize_from_string, StringSerdeError};

use crate::server_loop::compact_server_loop;

use proto::file::{IdentityFile, TrustedAppFile};

#[allow(clippy::enum_variant_names)]
#[derive(Debug, From)]
pub enum CompactBinError {
    CreateThreadPoolError,
    CreateTimerError,
    /*
    LoadIdentityError,
    LoadDbError,
    SpawnError,
    NetNodeError(NetNodeError),
    // SerializeError(SerializeError),
    StringSerdeError(StringSerdeError),
    IoError(std::io::Error),
    */
}

/// stcompact: Offst Compact
/// The decentralized credit payment engine
///
///『將欲奪之，必固與之』
///
#[derive(Debug, StructOpt)]
#[structopt(name = "stcompact")]
pub struct StCompactCmd {
    /// StCompact store path.
    /// If directory is nonexistent, a new store will be created.
    #[structopt(parse(from_os_str), short = "s", long = "store")]
    pub store: PathBuf,
}

pub fn stcompact(st_compact_cmd: StCompactCmd) -> Result<(), CompactBinError> {
    let StCompactCmd { store } = st_compact_cmd;

    /*
    // TODO:
    - Things to prepare:
        - Create version connector (Connector + version transform)
        - Store
        - async stdio
        - serialization (json)
            - Which seperator to use for messages? Newline?
    */

    // Create a ThreadPool:
    let thread_pool = ThreadPool::new().map_err(|_| CompactBinError::CreateThreadPoolError)?;

    // Create thread pool for file system operations:
    let file_system_thread_pool =
        ThreadPool::new().map_err(|_| CompactBinError::CreateThreadPoolError)?;

    // Get a timer client:
    let dur = Duration::from_millis(usize_to_u64(TICK_MS).unwrap());
    let timer_client =
        create_timer(dur, thread_pool.clone()).map_err(|_| CompactBinError::CreateTimerError)?;

    // A tcp connector, Used to connect to remote servers:
    let tcp_connector = TcpConnector::new(MAX_FRAME_LENGTH, thread_pool.clone());

    // Obtain secure cryptographic random:
    let rng = system_random();

    /*
    let store = unimplemented!();
    let conn_pair = unimplemented!();
    let version_connector = unimplemented!();

    let compact_server_fut = compact_server_loop(
        conn_pair, // : ConnPairCompactServer,
        store,
        timer_client,
        rng,
        version_connector,
        thread_pool,
    );
    */

    // block_on(node_fut).map_err(NodeBinError::NetNodeError)
    unimplemented!();
}
