use std::time::Duration;

use futures::channel::mpsc;
use futures::task::{Spawn, SpawnExt};
use futures::{SinkExt, StreamExt};

use common::conn::{ConnPair, ConnPairVec, FutTransform};
use common::int_convert::usize_to_u64;

use proto::consts::{KEEPALIVE_TICKS, PROTOCOL_VERSION, TICKS_TO_REKEY, MAX_FRAME_LENGTH, TICK_MS};
use proto::app_server::messages::{AppPermissions, AppServerToApp, AppToAppServer, NodeReport};
use proto::net::messages::NetAddress;
use proto::proto_ser::{ProtoDeserialize, ProtoSerialize};
use proto::crypto::PublicKey;

use crypto::rand::{system_random, CryptoRandom};

use identity::IdentityClient;
use net::TcpConnector;
use timer::{create_timer, TimerClient};

use keepalive::KeepAliveChannel;
use secure_channel::SecureChannel;
use version::VersionPrefix;

/// A connection of an App to a Node
pub type ConnPairApp = ConnPair<AppToAppServer, AppServerToApp>;

pub type AppConnTuple = (
    AppPermissions,
    NodeReport,
    ConnPairApp,
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
    spawner: S,
) -> Result<AppConnTuple, SetupConnectionError>
where
    R: Clone + CryptoRandom + 'static,
    S: Spawn + Clone + Send + 'static,
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
    let ver_conn = version_transform.transform(conn_pair).await;

    // Encrypt, requiring that the remote side will have node_public_key as public key:
    let (public_key, enc_conn) = encrypt_transform
        .transform((Some(node_public_key.clone()), ver_conn))
        .await
        .ok_or(SetupConnectionError::EncryptSetupError)?;
    assert_eq!(public_key, node_public_key);

    // Keepalive wrapper:
    let (mut sender, mut receiver) = keepalive_transform.transform(enc_conn).await.split();

    // Get AppPermissions:
    let app_permissions_data = receiver
        .next()
        .await
        .ok_or(SetupConnectionError::RecvAppPermissionsError)?;
    let app_permissions = AppPermissions::proto_deserialize(&app_permissions_data)
        .map_err(|_| SetupConnectionError::DeserializeAppPermissionsError)?;

    // Wait for the first NodeReport.
    let data = receiver
        .next()
        .await
        .ok_or(SetupConnectionError::ClosedBeforeNodeReport)?;
    let message = AppServerToApp::proto_deserialize(&data)
        .map_err(|_| SetupConnectionError::DeserializeNodeReportError)?;

    let node_report = if let AppServerToApp::Report(node_report) = message {
        node_report
    } else {
        return Err(SetupConnectionError::FirstMessageNotNodeReport);
    };

    // serialization:
    let (user_sender, mut from_user_sender) = mpsc::channel::<AppToAppServer>(0);

    // QUESTION: We have size of 1 to avoid deadlocks in tests. Not fully sure why a deadlock happens if we
    // put 0 instead.
    let (mut to_user_receiver, user_receiver) = mpsc::channel(1);

    // Deserialize data received from node:
    let _ = spawner.spawn(async move {
        while let Some(data) = receiver.next().await {
            let message = match AppServerToApp::proto_deserialize(&data) {
                Ok(message) => message,
                Err(_) => return,
            };
            if to_user_receiver.send(message).await.is_err() {
                return;
            }
        }
    });

    // Serialize data sent to node:
    let _ = spawner.spawn(async move {
        while let Some(message) = from_user_sender.next().await {
            // let data = serialize_app_to_app_server(&message);
            let data = message.proto_serialize();
            if sender.send(data).await.is_err() {
                return;
            }
        }
    });

    Ok((
        app_permissions,
        node_report,
        ConnPair::from_raw(user_sender, user_receiver),
    ))
}

#[derive(Debug)]
pub enum InnerConnectError {
    /// Could not open network connection
    TcpConnectorError,
    SetupConnectionError(SetupConnectionError),
}

/// Connect to an offst node
pub async fn inner_connect<C, R, S>(
    mut tcp_connector: C,
    node_public_key: PublicKey,
    node_net_address: NetAddress,
    timer_client: TimerClient,
    app_identity_client: IdentityClient,
    rng: R,
    spawner: S,
) -> Result<AppConnTuple, InnerConnectError>
where
    C: FutTransform<Input = NetAddress, Output = Option<ConnPairVec>>,
    R: CryptoRandom + Clone + 'static,
    S: Spawn + Send + Clone + 'static,
{
    let conn_pair = tcp_connector
        .transform(node_net_address)
        .await
        .ok_or(InnerConnectError::TcpConnectorError)?;

    setup_connection(
        conn_pair,
        timer_client,
        rng.clone(),
        node_public_key,
        app_identity_client,
        spawner.clone(),
    )
    .await
    .map_err(InnerConnectError::SetupConnectionError)
}


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
    // A tcp connector, Used to connect to remote servers:
    let tcp_connector = TcpConnector::new(MAX_FRAME_LENGTH, spawner.clone());

    // Get a timer client:
    let dur = Duration::from_millis(usize_to_u64(TICK_MS).unwrap());
    let timer_client = create_timer(dur, spawner.clone()).map_err(|_| ConnectError)?;

    // Obtain secure cryptographic random:
    let rng = system_random();

    inner_connect(
        tcp_connector,
        node_public_key,
        node_net_address,
        timer_client,
        app_identity_client,
        rng,
        spawner,
    )
    .await
    .map_err(|_| ConnectError)
}
