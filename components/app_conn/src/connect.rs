use futures::channel::mpsc;
use futures::task::{Spawn, SpawnExt};
use futures::{SinkExt, StreamExt};

use common::conn::{ConnPair, ConnPairVec, FutTransform};

use proto::app_server::messages::{AppPermissions, AppServerToApp, AppToAppServer, NodeReport};
use proto::crypto::PublicKey;
use proto::net::messages::NetAddress;
use proto::proto_ser::{ProtoDeserialize, ProtoSerialize};


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
async fn setup_connection<S>(
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
pub enum AppConnectError {
    /// Could not open network connection
    ConnectorError,
    SetupConnectionError(SetupConnectionError),
}

/// Connect to an offst node as an app
pub async fn app_connect_to_node<C, S>(
    mut connector: C,
    node_public_key: PublicKey,
    node_net_address: NetAddress,
    spawner: S,
) -> Result<AppConnTuple, AppConnectError>
where
    C: FutTransform<Input = (PublicKey, NetAddress), Output = Option<ConnPairVec>>,
    S: Spawn + Send + Clone + 'static,
{
    let conn_pair = connector
        .transform((node_public_key.clone(), node_net_address))
        .await
        .ok_or(AppConnectError::ConnectorError)?;

    setup_connection(
        conn_pair,
        spawner.clone(),
    )
    .await
    .map_err(AppConnectError::SetupConnectionError)
}
