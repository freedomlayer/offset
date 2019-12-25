use std::time::Duration;

use futures::channel::mpsc;
use futures::task::{Spawn, SpawnExt};
use futures::{SinkExt, StreamExt};

use common::conn::{ConnPair, ConnPairVec, FutTransform, FuncFutTransform};
use common::int_convert::usize_to_u64;

use proto::app_server::messages::{AppPermissions, AppServerToApp, AppToAppServer, NodeReport};
use proto::consts::{KEEPALIVE_TICKS, MAX_FRAME_LENGTH, PROTOCOL_VERSION, TICKS_TO_REKEY, TICK_MS};
use proto::crypto::PublicKey;
use proto::net::messages::NetAddress;
use proto::proto_ser::{ProtoDeserialize, ProtoSerialize};

use crypto::rand::{system_random, CryptoRandom};

use identity::IdentityClient;
use net::TcpConnector;
use timer::{create_timer, TimerClient};

use keepalive::KeepAliveChannel;
use secure_channel::SecureChannel;
use version::VersionPrefix;

/// A connection of an App to a Node
pub type ConnPairApp = ConnPair<AppToAppServer, AppServerToApp>;

pub type AppConnTuple = (AppPermissions, NodeReport, ConnPairApp);

#[derive(Debug)]
pub enum SetupConnectionError {
    EncryptSetupError,
    RecvAppPermissionsError,
    DeserializeAppPermissionsError,
    RecvNodeReportError,
    DeserializeNodeReportError,
    ClosedBeforeNodeReport,
    FirstMessageNotNodeReport,
}

/// Connect to an offst-node
pub async fn setup_connection<S>(
    conn_pair: ConnPairVec,
    spawner: S,
) -> Result<AppConnTuple, SetupConnectionError>
where
    S: Spawn + Clone + Send + 'static,
{
    let (mut sender, mut receiver) = conn_pair.split();

    // Get AppPermissions:
    let app_permissions_data = receiver
        .next()
        .await
        .ok_or(SetupConnectionError::RecvAppPermissionsError)?;
    let app_permissions = AppPermissions::proto_deserialize(&app_permissions_data)
        .map_err(|_| SetupConnectionError::DeserializeAppPermissionsError)?;

    // Get NodeReport:
    let node_report_data = receiver
        .next()
        .await
        .ok_or(SetupConnectionError::RecvNodeReportError)?;
    let node_report = NodeReport::proto_deserialize(&node_report_data)
        .map_err(|_| SetupConnectionError::DeserializeNodeReportError)?;

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
pub enum ConnectExError {
    /// Could not open network connection
    ConnectorError,
    SetupConnectionError(SetupConnectionError),
}

/// Connect to an offst node
pub async fn connect_ex<C, S>(
    mut connector: C,
    node_public_key: PublicKey,
    node_net_address: NetAddress,
    spawner: S,
) -> Result<AppConnTuple, ConnectExError>
where
    C: FutTransform<Input = (PublicKey, NetAddress), Output = Option<ConnPairVec>>,
    S: Spawn + Send + Clone + 'static,
{
    let conn_pair = connector
        .transform((node_public_key.clone(), node_net_address))
        .await
        .ok_or(ConnectExError::ConnectorError)?;

    setup_connection(
        conn_pair,
        spawner.clone(),
    )
    .await
    .map_err(ConnectExError::SetupConnectionError)
}

#[derive(Debug)]
pub struct ConnectError;


pub fn create_secure_connector<C, R, S>(
    connector: C,
    timer_client: TimerClient,
    identity_client: IdentityClient,
    rng: R,
    spawner: S) -> impl FutTransform<Input=(PublicKey, NetAddress), Output=Option<ConnPairVec>>
where
    S: Spawn + Clone + Send + 'static,
    R: CryptoRandom + Clone + 'static,
    C: FutTransform<Input=NetAddress, Output=Option<ConnPairVec>> + Clone + Send + 'static,
{
    // Wrap the connection (Version * Encrypt * Keepalive):
    let version_transform = VersionPrefix::new(PROTOCOL_VERSION, spawner.clone());
    let encrypt_transform = SecureChannel::new(
        identity_client,
        rng,
        timer_client.clone(),
        TICKS_TO_REKEY,
        spawner.clone(),
    );
    let keepalive_transform =
        KeepAliveChannel::new(timer_client.clone(), KEEPALIVE_TICKS, spawner.clone());

    let c_encrypt_transform = encrypt_transform.clone();
    let c_keepalive_transform = keepalive_transform.clone();
    FuncFutTransform::new(move |(public_key, net_address)| {
        let mut c_connector = connector.clone();
        let mut c_version_transform = version_transform.clone();
        let mut c_encrypt_transform = c_encrypt_transform.clone();
        let mut c_keepalive_transform = c_keepalive_transform.clone();
        Box::pin(async move {
            let conn_pair = c_connector.transform(net_address).await?;
            let conn_pair = c_version_transform.transform(conn_pair).await;
            let (_public_key, conn_pair) = c_encrypt_transform
                .transform((Some(public_key), conn_pair))
                .await?;
            let conn_pair = c_keepalive_transform.transform(conn_pair).await;
            Some(conn_pair)
        })
    })

}

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
        spawner.clone());

    connect_ex(
        secure_connector,
        node_public_key,
        node_net_address,
        spawner,
    )
    .await
    .map_err(|_| ConnectError)
}
