use std::path::PathBuf;
use std::time::Duration;

use derive_more::From;

// use futures::executor::ThreadPool;
#[allow(unused)]
use futures::task::{Spawn, SpawnExt};

use futures::channel::mpsc;
use futures::StreamExt;

use structopt::StructOpt;

// use common::conn::{FuncFutTransform, FutTransform};
use common::int_convert::usize_to_u64;

use crypto::rand::system_random;

use timer::create_timer;

use net::TcpConnector;
// use version::VersionPrefix;
// use connection::{create_encrypt_keepalive, create_secure_connector};

use proto::consts::{MAX_FRAME_LENGTH, TICK_MS};

#[allow(unused)]
use crate::server_loop::{compact_server_loop, ServerError, ConnPairCompactServer};
use crate::store::open_file_store;

#[allow(clippy::enum_variant_names)]
#[derive(Debug, From)]
pub enum CompactBinError {
    // CreateThreadPoolError,
    CreateTimerError,
    OpenFileStoreError,
    ServerError(ServerError),
    SpawnError,
    /*
    LoadIdentityError,
    LoadDbError,
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
fn create_stdio_conn_pair<S>(spawner: &S) -> Result<ConnPairCompactServer, CompactBinError>
where
    S: Spawn,
{
    let (user_sender, mut receiver) = mpsc::channel(1);
    let (_sender, user_receiver) = mpsc::channel(1);

    let mut stdout = async_std::io::stdout();
    let mut stdin = async_std::io::stdin();

    let send_fut = async move {
        // Send data to stdout:
        while let Some(user_to_server_ack) = receiver.next().await {
            todo!();
            // - Serialize
            // - Output to shell
        }
    };
    spawner.spawn(send_fut).map_err(|_| CompactBinError::SpawnError)?;

    let recv_fut = async move {
        // Receive data from stdin:
        let mut line = String::new();
        loop {
            // Read line from shell:
            if let Err(_) = stdin.read_line(&mut line).await {
                return;
            }
            todo!();
            // - Deserialize
            // - Forward to `sender`
        }
    };
    spawner.spawn(recv_fut).map_err(|_| CompactBinError::SpawnError)?;
    /*

    // TODO: Things to prepare:
        - async stdio
        - serialization (json)
            - Which seperator to use for messages? Newline?
    */
    Ok(ConnPairCompactServer::from_raw(user_sender, user_receiver))
}

#[allow(unused)]
pub async fn stcompact<S, FS>(
    st_compact_cmd: StCompactCmd,
    spawner: S,
    file_spawner: FS,
) -> Result<(), CompactBinError>
where
    S: Spawn + Clone + Send + Sync + 'static,
    FS: Spawn + Clone + Send + Sync + 'static,
{
    let StCompactCmd { store_path } = st_compact_cmd;

    // Get a timer client:
    let dur = Duration::from_millis(usize_to_u64(TICK_MS).unwrap());
    let timer_client =
        create_timer(dur, spawner.clone()).map_err(|_| CompactBinError::CreateTimerError)?;

    // A tcp connector, Used to connect to remote servers:
    let tcp_connector = TcpConnector::new(MAX_FRAME_LENGTH, spawner.clone());

    // Obtain secure cryptographic random:
    let rng = system_random();

    let file_store = open_file_store(store_path.into(), spawner.clone(), file_spawner.clone())
        .await
        .map_err(|_| CompactBinError::OpenFileStoreError)?;

    let conn_pair = create_stdio_conn_pair(&spawner)?;

    Ok(compact_server_loop(
        conn_pair,
        file_store,
        timer_client,
        rng,
        tcp_connector,
        spawner.clone(),
    ).await?)
}
