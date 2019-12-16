use std::path::PathBuf;
use std::time::Duration;

use derive_more::From;

// use futures::executor::ThreadPool;
#[allow(unused)]
use futures::task::{Spawn, SpawnExt};

use structopt::StructOpt;

use common::int_convert::usize_to_u64;
use common::conn::{FuncFutTransform, FutTransform};

use crypto::rand::system_random;

use timer::create_timer;

use net::TcpConnector;
use version::VersionPrefix;

use proto::consts::{MAX_FRAME_LENGTH, TICK_MS, PROTOCOL_VERSION};

#[allow(unused)]
use crate::server_loop::compact_server_loop;
use crate::store::open_file_store;

#[allow(clippy::enum_variant_names)]
#[derive(Debug, From)]
pub enum CompactBinError {
    // CreateThreadPoolError,
    CreateTimerError,
    OpenFileStoreError,
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
    pub store_path: PathBuf,
}

#[allow(unused)]
pub async fn stcompact<S, FS>(
    st_compact_cmd: StCompactCmd,
    spawner: S,
    file_spawner: FS,
) -> Result<(), CompactBinError>
where
    S: Spawn + Clone + Send + Sync + 'static,
    FS: Spawn + Clone,
{
    let StCompactCmd { store_path } = st_compact_cmd;

    // Get a timer client:
    let dur = Duration::from_millis(usize_to_u64(TICK_MS).unwrap());
    let _timer_client =
        create_timer(dur, spawner.clone()).map_err(|_| CompactBinError::CreateTimerError)?;

    // A tcp connector, Used to connect to remote servers:
    let tcp_connector = TcpConnector::new(MAX_FRAME_LENGTH, spawner.clone());

    // Obtain secure cryptographic random:
    let _rng = system_random();

    // Wrap net connector with a version prefix:
    let version_transform = VersionPrefix::new(PROTOCOL_VERSION, spawner.clone());
    let c_version_transform = version_transform.clone();
    let version_connector = FuncFutTransform::new(move |address| {
        let mut c_net_connector = tcp_connector.clone();
        let mut c_version_transform = c_version_transform.clone();
        Box::pin(async move {
            let conn_pair = c_net_connector.transform(address).await?;
            Some(c_version_transform.transform(conn_pair).await)
        })
    });

    let file_store = open_file_store(store_path.into(), spawner.clone(), file_spawner.clone())
        .await
        .map_err(|_| CompactBinError::OpenFileStoreError)?;

    /*
    // TODO: Things to prepare:
        - async stdio
        - serialization (json)
            - Which seperator to use for messages? Newline?
    */

    /*
    let conn_pair = unimplemented!();

    let compact_server_fut = compact_server_loop(
        conn_pair, // : ConnPairCompactServer,
        file_store,
        timer_client,
        rng,
        version_connector,
        thread_pool,
    );
    */

    // block_on(node_fut).map_err(NodeBinError::NetNodeError)
    unimplemented!();
}
