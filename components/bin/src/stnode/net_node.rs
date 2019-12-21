use std::fmt::Debug;

use futures::channel::{mpsc, oneshot};
use futures::future::RemoteHandle;
use futures::task::{Spawn, SpawnExt};
use futures::{FutureExt, SinkExt, Stream, StreamExt, TryFutureExt};

use common::conn::{BoxFuture, ConnPair, ConnPairVec, FuncFutTransform, FutTransform};
use common::transform_pool::transform_pool_loop;

use crypto::rand::CryptoRandom;

use identity::IdentityClient;

use database::DatabaseClient;

use proto::app_server::messages::{AppPermissions, AppServerToApp, AppToAppServer, NodeReport};
use proto::consts::{KEEPALIVE_TICKS, PROTOCOL_VERSION, TICKS_TO_REKEY};
use proto::crypto::PublicKey;
use proto::net::messages::NetAddress;
use proto::proto_ser::{ProtoDeserialize, ProtoSerialize};

use timer::TimerClient;

use keepalive::KeepAliveChannel;
use secure_channel::SecureChannel;
use version::VersionPrefix;

use node::{
    node, ConnPairServer, IncomingAppConnection, NodeConfig, NodeError, NodeMutation, NodeState,
};

#[derive(Debug)]
pub enum NetNodeError {
    CreateThreadPoolError,
    RequestPublicKeyError,
    SpawnError,
    DatabaseIdentityMismatch,
    NodeError(NodeError),
}

#[derive(Clone)]
struct AppConnTransform<VT, ET, KT, TA, S> {
    version_transform: VT,
    encrypt_transform: ET,
    keepalive_transform: KT,
    trusted_apps: TA,
    spawner: S,
}

impl<VT, ET, KT, TA, S> AppConnTransform<VT, ET, KT, TA, S> {
    fn new(
        version_transform: VT,
        encrypt_transform: ET,
        keepalive_transform: KT,
        trusted_apps: TA,
        spawner: S,
    ) -> Self {
        AppConnTransform {
            version_transform,
            encrypt_transform,
            keepalive_transform,
            trusted_apps,
            spawner,
        }
    }
}

impl<VT, ET, KT, TA, S> FutTransform for AppConnTransform<VT, ET, KT, TA, S>
where
    VT: FutTransform<Input = ConnPairVec, Output = ConnPairVec> + Clone + Send,
    ET: FutTransform<
            Input = (Option<PublicKey>, ConnPairVec),
            Output = Option<(PublicKey, ConnPairVec)>,
        > + Clone
        + Send,
    KT: FutTransform<Input = ConnPairVec, Output = ConnPairVec> + Clone + Send,
    TA: TrustedApps + Send + Clone,
    S: Spawn + Clone + Send + 'static,
{
    type Input = ConnPairVec;
    type Output = Option<IncomingAppConnection<NetAddress>>;

    fn transform(&mut self, conn_pair: Self::Input) -> BoxFuture<'_, Self::Output> {
        // let c_trusted_apps = self.trusted_apps.clone();
        Box::pin(async move {
            // Version prefix:
            let ver_conn = self.version_transform.transform(conn_pair).await;
            // Encrypt:
            let (public_key, enc_conn) = self.encrypt_transform.transform((None, ver_conn)).await?;

            // Obtain permissions for app (Or reject it if not trusted):
            let app_permissions: AppPermissions =
                self.trusted_apps.app_permissions(&public_key).await?;

            // Keepalive wrapper:
            let (mut sender, mut receiver) =
                self.keepalive_transform.transform(enc_conn).await.split();

            // Tell app about its permissions:
            sender.send(app_permissions.proto_serialize()).await.ok()?;

            let (report_sender, report_receiver) =
                oneshot::channel::<(NodeReport, oneshot::Sender<ConnPairServer<NetAddress>>)>();

            let spawner = self.spawner.clone();
            let c_spawner = self.spawner.clone();

            spawner
                .spawn(async move {
                    let _ = async move {
                        let (node_report, conn_sender) = report_receiver.await.ok()?;
                        sender.send(node_report.proto_serialize()).await.ok()?;

                        // serialization:
                        let (user_sender, mut from_user_sender) =
                            mpsc::channel::<AppServerToApp>(0);
                        let (mut to_user_receiver, user_receiver) = mpsc::channel(0);

                        // Deserialize received data
                        let _ = c_spawner.spawn(async move {
                            let _ = async move {
                                while let Some(data) = receiver.next().await {
                                    let message = AppToAppServer::proto_deserialize(&data).ok()?;
                                    to_user_receiver.send(message).await.ok()?;
                                }
                                Some(())
                            }
                                .await;
                        });

                        // Serialize sent data:
                        let _ = c_spawner.spawn(async move {
                            let _ = async move {
                                while let Some(message) = from_user_sender.next().await {
                                    // let data = serialize_app_server_to_app(&message);
                                    let data = message.proto_serialize();
                                    sender.send(data).await.ok()?;
                                }
                                Some(())
                            }
                                .await;
                        });

                        conn_sender
                            .send(ConnPair::from_raw(user_sender, user_receiver))
                            .ok()
                    }
                        .await;
                })
                .ok()?;

            Some(IncomingAppConnection {
                app_permissions: app_permissions.clone(),
                report_sender,
            })
        })
    }
}

fn transform_incoming_apps<IAC, R, TA, S>(
    incoming_app_raw_conns: IAC,
    identity_client: IdentityClient,
    rng: R,
    timer_client: TimerClient,
    trusted_apps: TA,
    max_concurrent_incoming_apps: usize,
    spawner: S,
) -> Result<
    (
        RemoteHandle<()>,
        mpsc::Receiver<IncomingAppConnection<NetAddress>>,
    ),
    NetNodeError,
>
where
    IAC: Stream<Item = ConnPairVec> + Unpin + Send + 'static,
    R: CryptoRandom + Clone + 'static,
    TA: TrustedApps + Send + Clone + 'static,
    S: Spawn + Clone + Send + 'static,
{
    let version_transform = VersionPrefix::new(PROTOCOL_VERSION, spawner.clone());

    let encrypt_transform = SecureChannel::new(
        identity_client,
        rng,
        timer_client.clone(),
        TICKS_TO_REKEY,
        spawner.clone(),
    );

    let keepalive_transform =
        KeepAliveChannel::new(timer_client, KEEPALIVE_TICKS, spawner.clone());

    let app_conn_transform = AppConnTransform::new(
        version_transform,
        encrypt_transform,
        keepalive_transform,
        trusted_apps,
        spawner.clone(),
    );

    let (incoming_apps_sender, incoming_apps) = mpsc::channel(0);

    // Apply transform over every incoming app connection:
    let pool_fut = transform_pool_loop(
        incoming_app_raw_conns,
        incoming_apps_sender,
        app_conn_transform,
        max_concurrent_incoming_apps,
        spawner.clone(),
    )
    .map_err(|e| error!("transform_pool_loop() error: {:?}", e))
    .map(|_| ());

    // We spawn with handle here to make sure that this
    // future is dropped when this async function ends.
    let pool_handle = spawner
        .spawn_with_handle(pool_fut)
        .map_err(|_| NetNodeError::SpawnError)?;

    Ok((pool_handle, incoming_apps))
}

pub trait TrustedApps {
    /// Get the permissions of an app. Returns None if the app is not trusted at all.
    fn app_permissions<'a>(
        &'a mut self,
        app_public_key: &'a PublicKey,
    ) -> BoxFuture<'a, Option<AppPermissions>>;
}

pub async fn net_node<IAC, C, R, TA, S>(
    incoming_app_raw_conns: IAC,
    connector: C,
    timer_client: TimerClient,
    identity_client: IdentityClient,
    rng: R,
    node_config: NodeConfig,
    trusted_apps: TA,
    node_state: NodeState<NetAddress>,
    database_client: DatabaseClient<NodeMutation<NetAddress>>,
    spawner: S,
) -> Result<(), NetNodeError>
where
    IAC: Stream<Item = ConnPairVec> + Unpin + Send + 'static,
    C: FutTransform<Input = NetAddress, Output = Option<ConnPairVec>> + Clone + Send + 'static,
    R: CryptoRandom + Clone + 'static,
    TA: TrustedApps + Send + Clone + 'static,
    S: Spawn + Clone + Send + 'static,
{
    // TODO: Move this number somewhere else?
    let max_concurrent_incoming_apps = 0x10;
    let (_pool_handle, incoming_apps) = transform_incoming_apps(
        incoming_app_raw_conns,
        identity_client.clone(),
        rng.clone(),
        timer_client.clone(),
        trusted_apps,
        max_concurrent_incoming_apps,
        spawner.clone(),
    )?;

    let version_transform = VersionPrefix::new(PROTOCOL_VERSION, spawner.clone());
    let encrypt_transform = SecureChannel::new(
        identity_client.clone(),
        rng.clone(),
        timer_client.clone(),
        TICKS_TO_REKEY,
        spawner.clone(),
    );
    let keepalive_transform =
        KeepAliveChannel::new(timer_client.clone(), KEEPALIVE_TICKS, spawner.clone());

    let c_encrypt_transform = encrypt_transform.clone();
    let c_keepalive_transform = keepalive_transform.clone();
    let secure_connector = FuncFutTransform::new(move |(public_key, net_address)| {
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
    });

    // Note that this transform does not contain the version prefix, as it is applied to a
    // connection between two nodes, relayed using a relay server.
    let encrypt_keepalive = FuncFutTransform::new(move |(opt_public_key, conn_pair_vec)| {
        let mut c_encrypt_transform = encrypt_transform.clone();
        let mut c_keepalive_transform = keepalive_transform.clone();
        Box::pin(async move {
            let (public_key, conn_pair_vec) = c_encrypt_transform
                .transform((opt_public_key, conn_pair_vec))
                .await?;
            let conn_pair_vec = c_keepalive_transform.transform(conn_pair_vec).await;
            Some((public_key, conn_pair_vec))
        })
    });

    node(
        node_config,
        identity_client,
        timer_client,
        node_state,
        database_client,
        secure_connector,
        encrypt_keepalive,
        incoming_apps,
        rng,
        spawner.clone(),
    )
    .await
    .map_err(NetNodeError::NodeError)
}
