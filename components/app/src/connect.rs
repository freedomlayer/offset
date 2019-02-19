use std::time::Duration;

use futures::task::Spawn;

use common::int_convert::usize_to_u64;
use common::conn::FutTransform;
use proto::consts::{TICK_MS, MAX_FRAME_LENGTH};
use proto::net::messages::NetAddress;

use timer::create_timer;
use crypto::identity::PublicKey;
use crypto::crypto_rand::{CryptoRandom, system_random};
use identity::IdentityClient;

use net::NetConnector;

use crate::node_connection::NodeConnection;

use crate::setup_conn::{setup_connection, SetupConnectionError};
pub use crate::setup_conn::NodeConnectionTuple;



#[derive(Debug)]
pub enum ConnectError {
    CreateTimerError,
    CreateNetConnectorError,
    /// Could not open network connection
    NetConnectorError,
    SetupConnectionError(SetupConnectionError),
    CreateNodeConnectionError,
}

/// Connect to an offst-node
async fn inner_node_connect<R,S>(node_public_key: PublicKey,
                            net_address: NetAddress,
                            app_identity_client: IdentityClient,
                            rng: R,
                            spawner: S) -> Result<NodeConnectionTuple, ConnectError>
where
    R: CryptoRandom + Clone + 'static,
    S: Spawn + Clone + Sync + Send + 'static,
{
    // Get a timer client:
    let dur = Duration::from_millis(usize_to_u64(TICK_MS).unwrap()); 
    let timer_client = create_timer(dur, spawner.clone())
        .map_err(|_| ConnectError::CreateTimerError)?;

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


/// Connect to an offst node
pub async fn node_connect<C,S>(node_public_key: PublicKey,
                               net_address: NetAddress,
                               app_identity_client: IdentityClient,
                               mut spawner: S) -> Result<NodeConnection<impl CryptoRandom>, ConnectError> 
where
    S: Spawn + Send + Sync + Clone + 'static,
{

    // TODO: Is it ok to create a new system_random() on every call to node_connect()?
    // What are the alternatives?
    
    // Obtain secure cryptographic random:
    let rng = system_random();

    let conn_tuple = await!(inner_node_connect(node_public_key,
                       net_address,
                       app_identity_client,
                       rng.clone(),
                       spawner.clone()))?;

    NodeConnection::new(conn_tuple, rng, &mut spawner)
       .map_err(|_| ConnectError::CreateNodeConnectionError)
}
