use std::fmt::Debug;
use std::collections::HashMap;

use futures::executor::ThreadPool;
use futures::channel::mpsc;
use futures::{future, FutureExt, TryFutureExt, 
    Stream, StreamExt, SinkExt};
use futures::task::{Spawn, SpawnExt};

use common::conn::{FutTransform, ConnPairVec, 
    BoxFuture, FuncFutTransform};
use common::transform_pool::transform_pool_loop;

use crypto::identity::PublicKey;
use crypto::crypto_rand::CryptoRandom;

use proto::consts::{KEEPALIVE_TICKS, TICKS_TO_REKEY, PROTOCOL_VERSION};
use proto::net::messages::NetAddress;
use proto::app_server::messages::AppPermissions;
use proto::app_server::serialize::{serialize_app_server_to_app, 
    deserialize_app_to_app_server, serialize_app_permissions};

use timer::TimerClient;
use identity::IdentityClient;
use database::{database_loop, AtomicDb, DatabaseClient};

use version::VersionPrefix;
use secure_channel::SecureChannel;
use keepalive::KeepAliveChannel;
use app_server::IncomingAppConnection;

use crate::types::{NodeConfig, NodeState, NodeMutation};
use crate::node::{node, NodeError};

#[derive(Debug)]
pub enum NetNodeError {
    CreateThreadPoolError,
    RequestPublicKeyError,
    SpawnError,
    DatabaseIdentityMismatch,
    NodeError(NodeError),
}

#[derive(Clone)]
struct AppConnTransform<VT,ET,KT,S,GT> {
    version_transform: VT, 
    encrypt_transform: ET, 
    keepalive_transform: KT,
    spawner: S,
    get_trusted_apps: GT,
    thread_pool: ThreadPool,
}

impl<VT,ET,KT,S,GT> AppConnTransform<VT,ET,KT,S,GT> {
    fn new(version_transform: VT,
               encrypt_transform: ET,
               keepalive_transform: KT,
               spawner: S,
               get_trusted_apps: GT) -> Result<Self, NetNodeError> {

        // Create a extra inner thread pool.
        // We use another thread pool because we don't want to block
        // the main thread pool when we are reading files from the filesystem.
        // TODO: What is the idiomatic way to do this without having an extra ThreadPool?
        let thread_pool = ThreadPool::new()
            .map_err(|_| NetNodeError::CreateThreadPoolError)?;

        Ok(AppConnTransform {
            version_transform, 
            encrypt_transform, 
            keepalive_transform,
            spawner,
            get_trusted_apps,
            thread_pool,
        })
    }
}

impl<VT,ET,KT,S,GT> FutTransform for AppConnTransform<VT,ET,KT,S,GT> 
where
    VT: FutTransform<Input=ConnPairVec, Output=ConnPairVec> + Clone + Send,
    ET: FutTransform<Input=(Option<PublicKey>, ConnPairVec),
                     Output=Option<(PublicKey, ConnPairVec)>> + Clone + Send,
    KT: FutTransform<Input=ConnPairVec, Output=ConnPairVec> + Clone + Send,
    S: Spawn + Clone + Send,
    GT: Fn() -> Option<HashMap<PublicKey, AppPermissions>> + Clone + Send + 'static,
{
    type Input = ConnPairVec;
    type Output = Option<IncomingAppConnection<NetAddress>>;

    fn transform(&mut self, conn_pair: Self::Input)
        -> BoxFuture<'_, Self::Output> {

        Box::pin(async move {
            // Version perfix:
            let ver_conn = await!(self.version_transform.transform(conn_pair));
            // Encrypt:
            let (public_key, enc_conn) = await!(self.encrypt_transform.transform((None, ver_conn)))?;

            // Obtain permissions for app (Or reject it if not trusted):
            let c_get_trusted_apps = self.get_trusted_apps.clone();

            // Obtain trusted apps using a separate thread pool:
            // At this point we re-read the directory of all trusted apps.
            // This could be slow, therefore we perform this operation on self.thread_pool
            // and not on self.spawner, which is the main thread_pool for this program.
            let trusted_apps_fut = self.thread_pool.spawn_with_handle(
                future::lazy(move |_| (c_get_trusted_apps)())).ok()?;
            let trusted_apps = await!(trusted_apps_fut)?;

            let app_permissions = trusted_apps.get(&public_key)?;

            // Keepalive wrapper:
            let (mut sender, mut receiver) = await!(self.keepalive_transform.transform(enc_conn));

            // Tell app about its permissions: (TODO: Is this required?)
            await!(sender.send(serialize_app_permissions(&app_permissions))).ok()?;

            // serialization:
            let (user_sender, mut from_user_sender) = mpsc::channel(0);
            let (mut to_user_receiver, user_receiver) = mpsc::channel(0);

            // Deserialize received data
            let _ = self.spawner.spawn(async move {
                while let Some(data) = await!(receiver.next()) {
                    let message = match deserialize_app_to_app_server(&data) {
                        Ok(message) => message,
                        Err(_) => return,
                    };
                    if let Err(_) = await!(to_user_receiver.send(message)) {
                        return;
                    }
                }
            });

            // Serialize sent data:
            let _ = self.spawner.spawn(async move {
                while let Some(message) = await!(from_user_sender.next()) {
                    let data = serialize_app_server_to_app(&message);
                    if let Err(_) = await!(sender.send(data)) {
                        return;
                    }
                }
            });

            Some((app_permissions.clone(), (user_sender, user_receiver)))
        })
    }
}



pub async fn net_node<IAC,C,R,GT,AD,S>(incoming_app_raw_conns: IAC,
                      net_connector: C,
                      timer_client: TimerClient,
                      identity_client: IdentityClient,
                      rng: R,
                      node_config: NodeConfig,
                      get_trusted_apps: GT,
                      atomic_db: AD,
                      max_concurrent_incoming_app: usize,
                      mut spawner: S) -> Result<(), NetNodeError> 
where
    IAC: Stream<Item=ConnPairVec> + Unpin + Send + 'static,
    C: FutTransform<Input=NetAddress,Output=Option<ConnPairVec>> + Clone + Send + Sync + 'static,
    R: CryptoRandom + Clone + 'static,
    GT: Fn() -> Option<HashMap<PublicKey, AppPermissions>> + Clone + Send + 'static,
    AD: AtomicDb<State=NodeState<NetAddress>,Mutation=NodeMutation<NetAddress>> + Send + 'static,
    AD::Error: Send + Debug,
    S: Spawn + Clone + Send + Sync + 'static,
{
    // Wrap net connector with a version prefix:
    let version_transform = VersionPrefix::new(PROTOCOL_VERSION, 
                                                   spawner.clone());
    let c_version_transform = version_transform.clone();
    let version_connector = FuncFutTransform::new(move |address| {
        let mut c_net_connector = net_connector.clone();
        let mut c_version_transform = c_version_transform.clone();
        Box::pin(async move {
            let conn_pair = await!(c_net_connector.transform(address))?;
            Some(await!(c_version_transform.transform(conn_pair)))
        })
    });

    let local_public_key = await!(identity_client.request_public_key())
        .map_err(|_| NetNodeError::RequestPublicKeyError)?;

    // Get initial node_state:
    let node_state = atomic_db.get_state().clone();

    // Make sure that the local public key in the database
    // matches the local public key from the provided identity file:
    if node_state.funder_state.local_public_key != local_public_key {
        return Err(NetNodeError::DatabaseIdentityMismatch);
    }

    // Spawn database service:
    let (db_request_sender, incoming_db_requests) = mpsc::channel(0);
    let loop_fut = database_loop(atomic_db, incoming_db_requests)
        .map_err(|e| error!("database_loop() error: {:?}", e))
        .map(|_| ());
    spawner.spawn(loop_fut)
        .map_err(|_| NetNodeError::SpawnError)?;

    // Obtain a client to the database service:
    let database_client = DatabaseClient::new(db_request_sender);

    let encrypt_transform = SecureChannel::new(
        identity_client.clone(),
        rng.clone(),
        timer_client.clone(),
        TICKS_TO_REKEY,
        spawner.clone());

    let keepalive_transform = KeepAliveChannel::new(
        timer_client.clone(),
        KEEPALIVE_TICKS,
        spawner.clone());

    let app_conn_transform = AppConnTransform::new(
        version_transform,
        encrypt_transform,
        keepalive_transform,
        spawner.clone(),
        get_trusted_apps)?;

    let (incoming_apps_sender, incoming_apps) = mpsc::channel(0);

    // Apply transform over every incoming app connection:
    let pool_fut = transform_pool_loop(
            incoming_app_raw_conns,
            incoming_apps_sender,
            app_conn_transform,
            max_concurrent_incoming_app,
            spawner.clone())
        .map_err(|e| error!("transform_pool_loop() error: {:?}", e))
        .map(|_| ());

    spawner.spawn(pool_fut)
        .map_err(|_| NetNodeError::SpawnError)?;
    
    await!(node(
        node_config,
        identity_client,
        timer_client,
        node_state,
        database_client,
        version_connector,
        incoming_apps,
        rng,
        spawner.clone()))
            .map_err(|e| NetNodeError::NodeError(e))
}
