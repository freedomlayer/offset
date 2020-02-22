use std::path::PathBuf;
use std::time::Duration;

use derive_more::From;

use futures::task::{Spawn, SpawnExt};

use futures::channel::mpsc;
use futures::AsyncWriteExt;
use futures::{FutureExt, SinkExt, StreamExt};

use structopt::StructOpt;

use common::conn::ConnPairString;
use common::int_convert::usize_to_u64;

use crypto::rand::system_random;

use timer::create_timer;

use net::TcpConnector;

use proto::consts::{MAX_FRAME_LENGTH, TICK_MS};

use crate::serialize::{serialize_conn_pair, SerializeConnError};
use crate::server_loop::{compact_server_loop, ServerError};
use crate::store::open_file_store;

/// Amount of ticks to wait for the next attempt to reconnect to a remote node
const TICKS_TO_CONNECT: usize = 8;

#[allow(clippy::enum_variant_names)]
#[derive(Debug, From)]
pub enum StCompactError {
    CreateTimerError,
    OpenFileStoreError,
    ServerError(ServerError),
    SerializeConnError(SerializeConnError),
    SpawnError,
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

fn create_stdio_conn_pair<S>(spawner: &S) -> Result<ConnPairString, StCompactError>
where
    S: Spawn,
{
    let mut stdout = async_std::io::stdout();
    let stdin = async_std::io::stdin();

    let (server_sender, mut receiver) = mpsc::channel::<String>(1);
    let (mut sender, server_receiver) = mpsc::channel::<String>(1);

    let send_fut = async move {
        // Send data to stdout:
        while let Some(line) = receiver.next().await {
            // Output to shell:
            stdout.write_all(line.as_bytes()).await.ok()?;
            // TODO: Do we need to add a newline?
            stdout.write_all(b"\n").await.ok()?;
        }
        Some(())
    };
    spawner
        .spawn(send_fut.map(|_: Option<()>| ()))
        .map_err(|_| StCompactError::SpawnError)?;

    let recv_fut = async move {
        // Receive data from stdin:
        let mut line = String::new();
        loop {
            // Read line from shell:
            stdin.read_line(&mut line).await.ok()?;
            // Note: The line contains an extra newline:
            // TODO: Should we remove the trailing newline?
            if let Some(last) = line.chars().last() {
                assert!(last == '\n');
            }
            // Forward to user:
            sender.send(line.clone()).await.ok()?;
        }
    };
    spawner
        .spawn(recv_fut.map(|_: Option<()>| ()))
        .map_err(|_| StCompactError::SpawnError)?;

    Ok(ConnPairString::from_raw(server_sender, server_receiver))
}

pub async fn stcompact<S, FS>(
    st_compact_cmd: StCompactCmd,
    spawner: S,
    file_spawner: FS,
) -> Result<(), StCompactError>
where
    S: Spawn + Clone + Send + Sync + 'static,
    FS: Spawn + Clone + Send + Sync + 'static,
{
    let StCompactCmd { store_path } = st_compact_cmd;

    // Get a timer client:
    let dur = Duration::from_millis(usize_to_u64(TICK_MS).unwrap());
    let timer_client =
        create_timer(dur, spawner.clone()).map_err(|_| StCompactError::CreateTimerError)?;

    // A tcp connector, Used to connect to remote servers:
    let tcp_connector = TcpConnector::new(MAX_FRAME_LENGTH, spawner.clone());

    // Obtain secure cryptographic random:
    let rng = system_random();

    let file_store = open_file_store(store_path, spawner.clone(), file_spawner.clone())
        .await
        .map_err(|_| StCompactError::OpenFileStoreError)?;

    // Get line (string) communication with stdio:
    let stdio_conn_pair = create_stdio_conn_pair(&spawner)?;

    // Serialize communication:
    let conn_pair = serialize_conn_pair(stdio_conn_pair, &spawner)?;

    Ok(compact_server_loop(
        conn_pair,
        file_store,
        TICKS_TO_CONNECT,
        timer_client,
        rng,
        tcp_connector,
        spawner.clone(),
    )
    .await?)
}
