use std::time::Duration;

use futures::task::Spawn;

use common::int_convert::usize_to_u64;
use common::conn::FutTransform;
use proto::consts::{TICK_MS, MAX_FRAME_LENGTH};
use proto::net::messages::NetAddress;

use timer::create_timer;
use crypto::identity::PublicKey;
use crypto::crypto_rand::system_random;
use identity::IdentityClient;

use net::NetConnector;

use crate::setup_conn::{setup_connection, SetupConnectionError};
pub use crate::setup_conn::NodeConnection;



#[derive(Debug)]
pub enum ConnectError {
    CreateTimerError,
    CreateNetConnectorError,
    /// Could not open network connection
    NetConnectorError,
    SetupConnectionError(SetupConnectionError),
}

/// Connect to an offst-node
pub async fn connect<C,R,S>(node_public_key: PublicKey,
                            net_address: NetAddress,
                            app_identity_client: IdentityClient,
                            spawner: S) -> Result<NodeConnection, ConnectError>
where
    S: Spawn + Clone + Sync + Send + 'static,
{
    // Get a timer client:
    let dur = Duration::from_millis(usize_to_u64(TICK_MS).unwrap()); 
    let timer_client = create_timer(dur, spawner.clone())
        .map_err(|_| ConnectError::CreateTimerError)?;

    // Obtain secure cryptographic random:
    let rng = system_random();

    // A tcp connector, Used to connect to remote servers:
    let mut net_connector = NetConnector::new(MAX_FRAME_LENGTH, spawner.clone())
        .map_err(|_| ConnectError::CreateNetConnectorError)?;

    let conn_pair = await!(net_connector.transform(net_address))
        .ok_or(ConnectError::NetConnectorError)?;

    await!(setup_connection(
                conn_pair,
                timer_client,
                rng,
                node_public_key,
                app_identity_client,
                spawner))
        .map_err(|e| ConnectError::SetupConnectionError(e))
}
