use std::time::Duration;

use futures::task::Spawn;

use common::conn::ConnPair;
use common::int_convert::usize_to_u64;

use proto::app_server::messages::{AppPermissions, AppServerToApp, AppToAppServer, NodeReport};
use proto::consts::MAX_FRAME_LENGTH;
use proto::consts::TICK_MS;
use proto::crypto::PublicKey;
use proto::net::messages::NetAddress;

use crypto::rand::system_random;

use identity::IdentityClient;
use net::TcpConnector;
use timer::create_timer;

use app_conn::app_connect_to_node;
use connection::create_secure_connector;

/// A connection of an App to a Node
pub type ConnPairApp = ConnPair<AppToAppServer, AppServerToApp>;

pub type AppConnTuple = (AppPermissions, NodeReport, ConnPairApp);

#[derive(Debug)]
pub struct ConnectError;

/// Connect to a remote offst-node.
pub async fn connect<S>(
    node_public_key: PublicKey,
    node_net_address: NetAddress,
    app_identity_client: IdentityClient,
    spawner: S,
) -> Result<AppConnTuple, ConnectError>
where
    S: Spawn + Clone + Send + 'static,
{
    // Obtain secure cryptographic random:
    let rng = system_random();

    // Get a timer client:
    let dur = Duration::from_millis(usize_to_u64(TICK_MS).unwrap());
    let timer_client = create_timer(dur, spawner.clone()).map_err(|_| ConnectError)?;

    // A tcp connector, Used to connect to remote servers:
    let tcp_connector = TcpConnector::new(MAX_FRAME_LENGTH, spawner.clone());

    let secure_connector = create_secure_connector(
        tcp_connector,
        timer_client,
        app_identity_client,
        rng,
        spawner.clone(),
    );

    app_connect_to_node(secure_connector, node_public_key, node_net_address, spawner)
        .await
        .map_err(|_| ConnectError)
}
