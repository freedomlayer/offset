use futures::channel::mpsc;
use futures::task::{Spawn, SpawnExt};
use futures::{SinkExt, StreamExt};

use common::conn::{ConnPair, ConnPairVec, FutTransform};

use proto::app_server::messages::{AppPermissions, AppServerToApp, AppToAppServer, NodeReport};
use proto::app_server::serialize::{
    deserialize_app_permissions, deserialize_app_server_to_app, serialize_app_to_app_server,
};
use proto::consts::{KEEPALIVE_TICKS, PROTOCOL_VERSION, TICKS_TO_REKEY};
use proto::net::messages::NetAddress;

use timer::TimerClient;

use crypto::crypto_rand::CryptoRandom;
use crypto::identity::PublicKey;
use identity::IdentityClient;

pub use super::node_connection::NodeConnection;

use keepalive::KeepAliveChannel;
use secure_channel::SecureChannel;
use version::VersionPrefix;

pub type NodeConnectionTuple = (
    AppPermissions,
    NodeReport,
    ConnPair<AppToAppServer, AppServerToApp>,
);

#[derive(Debug)]
pub enum SetupConnectionError {
    EncryptSetupError,
    RecvAppPermissionsError,
    DeserializeAppPermissionsError,
    ClosedBeforeNodeReport,
    DeserializeNodeReportError,
    FirstMessageNotNodeReport,
}

/// Connect to an offst-node
pub async fn setup_connection<R, S>(
    conn_pair: ConnPairVec,
    timer_client: TimerClient,
    rng: R,
    node_public_key: PublicKey,
    app_identity_client: IdentityClient,
    mut spawner: S,
) -> Result<NodeConnectionTuple, SetupConnectionError>
where
    R: Clone + CryptoRandom + 'static,
    S: Spawn + Clone + Send + Sync + 'static,
{
    let mut version_transform = VersionPrefix::new(PROTOCOL_VERSION, spawner.clone());

    let mut encrypt_transform = SecureChannel::new(
        app_identity_client.clone(),
        rng.clone(),
        timer_client.clone(),
        TICKS_TO_REKEY,
        spawner.clone(),
    );

    let mut keepalive_transform =
        KeepAliveChannel::new(timer_client.clone(), KEEPALIVE_TICKS, spawner.clone());

    // Report version and check remote side's version:
    let ver_conn = await!(version_transform.transform(conn_pair));

    // Encrypt, requiring that the remote side will have node_public_key as public key:
    let (public_key, enc_conn) =
        await!(encrypt_transform.transform((Some(node_public_key.clone()), ver_conn)))
            .ok_or(SetupConnectionError::EncryptSetupError)?;
    assert_eq!(public_key, node_public_key);

    // Keepalive wrapper:
    let (mut sender, mut receiver) = await!(keepalive_transform.transform(enc_conn));

    // Get AppPermissions:
    let app_permissions_data =
        await!(receiver.next()).ok_or(SetupConnectionError::RecvAppPermissionsError)?;
    let app_permissions = deserialize_app_permissions(&app_permissions_data)
        .map_err(|_| SetupConnectionError::DeserializeAppPermissionsError)?;

    // Wait for the first NodeReport.
    let data = await!(receiver.next()).ok_or(SetupConnectionError::ClosedBeforeNodeReport)?;
    let message = deserialize_app_server_to_app(&data)
        .map_err(|_| SetupConnectionError::DeserializeNodeReportError)?;

    let node_report = if let AppServerToApp::Report(node_report) = message {
        node_report
    } else {
        return Err(SetupConnectionError::FirstMessageNotNodeReport)?;
    };

    // serialization:
    let (user_sender, mut from_user_sender) = mpsc::channel(0);
    let (mut to_user_receiver, user_receiver) = mpsc::channel(0);

    // Deserialize data received from node:
    let _ = spawner.spawn(async move {
        while let Some(data) = await!(receiver.next()) {
            let message = match deserialize_app_server_to_app(&data) {
                Ok(message) => message,
                Err(_) => return,
            };
            if let Err(_) = await!(to_user_receiver.send(message)) {
                return;
            }
        }
    });

    // Serialize data sent to node:
    let _ = spawner.spawn(async move {
        while let Some(message) = await!(from_user_sender.next()) {
            let data = serialize_app_to_app_server(&message);
            if let Err(_) = await!(sender.send(data)) {
                return;
            }
        }
    });

    Ok((app_permissions, node_report, (user_sender, user_receiver)))
}

#[derive(Debug)]
pub enum NodeConnectError {
    /// Could not open network connection
    NetConnectorError,
    SetupConnectionError(SetupConnectionError),
    CreateNodeConnectionError,
}

/// Connect to an offst node
pub async fn node_connect<C, R, S>(
    mut net_connector: C,
    node_public_key: PublicKey,
    node_net_address: NetAddress,
    timer_client: TimerClient,
    app_identity_client: IdentityClient,
    rng: R,
    mut spawner: S,
) -> Result<NodeConnection<R>, NodeConnectError>
where
    C: FutTransform<Input = NetAddress, Output = Option<ConnPairVec>>,
    R: CryptoRandom + Clone + 'static,
    S: Spawn + Send + Sync + Clone + 'static,
{
    let conn_pair = await!(net_connector.transform(node_net_address))
        .ok_or(NodeConnectError::NetConnectorError)?;

    let conn_tuple = await!(setup_connection(
        conn_pair,
        timer_client,
        rng.clone(),
        node_public_key,
        app_identity_client,
        spawner.clone()
    ))
    .map_err(|e| NodeConnectError::SetupConnectionError(e))?;

    NodeConnection::new(conn_tuple, rng, &mut spawner)
        .map_err(|_| NodeConnectError::CreateNodeConnectionError)
}
