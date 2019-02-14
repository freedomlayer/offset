use futures::task::{Spawn, SpawnExt};
use futures::channel::mpsc;
use futures::{StreamExt, SinkExt};

use common::conn::{ConnPair, ConnPairVec, FutTransform};
use crypto::identity::PublicKey;
use crypto::crypto_rand::CryptoRandom;
use identity::IdentityClient;
use timer::TimerClient;

use proto::app_server::messages::{AppToAppServer, AppServerToApp, AppPermissions};
use proto::app_server::serialize::{deserialize_app_server_to_app,
                                   serialize_app_to_app_server,
                                   deserialize_app_permissions};
// use proto::net::messages::NetAddress;
use proto::consts::{PROTOCOL_VERSION, TICKS_TO_REKEY, KEEPALIVE_TICKS};

use secure_channel::SecureChannel;
use version::VersionPrefix;
use keepalive::KeepAliveChannel;


pub type NodeConnection = (AppPermissions, ConnPair<AppToAppServer,AppServerToApp>);

#[derive(Debug)]
pub enum SetupConnectionError {
    EncryptSetupError,
    RecvAppPermissionsError,
    DeserializeAppPermissionsError,
}

/// Connect to an offst-node
pub async fn setup_connection<R,S>(
                    conn_pair: ConnPairVec,
                    timer_client: TimerClient,
                    rng: R,
                    node_public_key: PublicKey,
                    app_identity_client: IdentityClient,
                    mut spawner: S) -> Result<NodeConnection, SetupConnectionError>
where   
    R: Clone + CryptoRandom + 'static,
    S: Spawn + Clone + Send + Sync + 'static,
{
    let mut version_transform = VersionPrefix::new(PROTOCOL_VERSION, 
                                               spawner.clone());

    let mut encrypt_transform = SecureChannel::new(
        app_identity_client.clone(),
        rng.clone(),
        timer_client.clone(),
        TICKS_TO_REKEY,
        spawner.clone());

    let mut keepalive_transform = KeepAliveChannel::new(
        timer_client.clone(),
        KEEPALIVE_TICKS,
        spawner.clone());

    // Report version and check remote side's version:
    let ver_conn = await!(version_transform.transform(conn_pair));

    // Encrypt, requiring that the remote side will have node_public_key as public key:
    let (public_key, enc_conn) = await!(encrypt_transform.transform(
            (Some(node_public_key.clone()), ver_conn)))
        .ok_or(SetupConnectionError::EncryptSetupError)?;
    assert_eq!(public_key, node_public_key);

    // Keepalive wrapper:
    let (mut sender, mut receiver) = await!(keepalive_transform.transform(enc_conn));

    // Get AppPermissions:
    let app_permissions_data = await!(receiver.next())
        .ok_or(SetupConnectionError::RecvAppPermissionsError)?;
    let app_permissions = deserialize_app_permissions(&app_permissions_data)
        .map_err(|_| SetupConnectionError::DeserializeAppPermissionsError)?;

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

    Ok((app_permissions, (user_sender, user_receiver)))
}
