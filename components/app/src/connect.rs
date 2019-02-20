use std::time::Duration;

use futures::task::Spawn;

use common::int_convert::usize_to_u64;

use proto::consts::{TICK_MS, MAX_FRAME_LENGTH};
use proto::net::messages::NetAddress;

use crypto::crypto_rand::{CryptoRandom, system_random};
use crypto::identity::PublicKey;

use identity::IdentityClient;
use timer::create_timer;
use net::NetConnector;

use node::connect::{node_connect, NodeConnection};

#[derive(Debug)]
pub struct ConnectError;

/// Connect to a remote offst-node.
pub async fn connect<S>(node_public_key: PublicKey,
                     node_net_address: NetAddress,
                     app_identity_client: IdentityClient,
                     spawner: S) -> Result<NodeConnection<impl CryptoRandom>, ConnectError>
where
    S: Spawn + Clone + Send + Sync + 'static,
{
    // A tcp connector, Used to connect to remote servers:
    let net_connector = NetConnector::new(MAX_FRAME_LENGTH, spawner.clone())
        .map_err(|_| ConnectError)?;

    // Get a timer client:
    let dur = Duration::from_millis(usize_to_u64(TICK_MS).unwrap()); 
    let timer_client = create_timer(dur, spawner.clone())
        .map_err(|_| ConnectError)?;

    // TODO: Is it safe to create a new `system_random` every time?
    // Obtain secure cryptographic random:
    let rng = system_random();

    await!(node_connect(net_connector,
                 node_public_key,
                 node_net_address,
                 timer_client,
                 app_identity_client,
                 rng,
                 spawner))
        .map_err(|_| ConnectError)
}
