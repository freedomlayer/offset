use std::fmt::Debug;
use std::marker::Unpin;
use std::collections::HashMap;

use futures::task::{Spawn, SpawnExt};
use futures::{FutureExt, TryFutureExt, Stream, StreamExt, SinkExt};
use futures::channel::mpsc;

use common::conn::{FutTransform, ConnPair, ConnPairVec, 
    BoxFuture, FuncFutTransform};
use common::transform_pool::transform_pool_loop;

use proto::consts::{PROTOCOL_VERSION, INDEX_NODE_TIMEOUT_TICKS, TICKS_TO_REKEY,
                    KEEPALIVE_TICKS};
use proto::index_server::serialize::{deserialize_index_server_to_server,
                                        serialize_index_server_to_server,
                                        serialize_index_server_to_client,
                                        deserialize_index_client_to_server};
use proto::index_server::messages::{IndexServerToServer, IndexClientToServer,
                                    IndexServerToClient};

use timer::TimerClient;

use crypto::identity::{PublicKey, compare_public_key};
use crypto::crypto_rand::CryptoRandom;

use identity::IdentityClient;
use version::VersionPrefix;
use secure_channel::SecureChannel;
use keepalive::KeepAliveChannel;

use crate::server::{server_loop, ServerLoopError};
pub use crate::server::{ServerConn, ClientConn};

use crate::verifier::simple_verifier::SimpleVerifier;
use crate::graph::graph_service::create_graph_service;
use crate::graph::simple_capacity_graph::SimpleCapacityGraph;
use crate::backoff_connector::BackoffConnector;


#[derive(Debug)]
pub enum IndexServerError {
    RequestTimerStreamError,
    CreateGraphServiceError,
    ServerLoopError(ServerLoopError),
}


/// Run an index server
/// Will keep running until an error occurs.
async fn index_server<A,IS,IC,SC,R,S>(local_public_key: PublicKey,
                           trusted_servers: HashMap<PublicKey, A>, 
                           incoming_server_connections: IS,
                           incoming_client_connections: IC,
                           server_connector: SC,
                           mut timer_client: TimerClient,
                           ticks_to_live: usize,
                           backoff_ticks: usize,
                           rng: R,
                           spawner: S) 
                                -> Result<(), IndexServerError>
where
    A: Debug + Send + Clone + 'static,
    IS: Stream<Item=(PublicKey, ServerConn)> + Unpin,
    IC: Stream<Item=(PublicKey, ClientConn)> + Unpin,
    SC: FutTransform<Input=(PublicKey, A), Output=Option<ServerConn>> + Clone + Send + 'static,
    R: CryptoRandom,
    S: Spawn + Clone + Send,
{

    let verifier = SimpleVerifier::new(ticks_to_live, rng);

    let capacity_graph = SimpleCapacityGraph::new();
    let graph_client = create_graph_service(capacity_graph, spawner.clone())
        .map_err(|_| IndexServerError::CreateGraphServiceError)?;

    let timer_stream = await!(timer_client.request_timer_stream())
        .map_err(|_| IndexServerError::RequestTimerStreamError)?;

    let backoff_connector = BackoffConnector::new(server_connector,
                                                  timer_client,
                                                  backoff_ticks);

    await!(server_loop(local_public_key, 
                trusted_servers,
                incoming_server_connections,
                incoming_client_connections,
                backoff_connector,
                graph_client,
                compare_public_key,
                verifier,
                timer_stream,
                spawner,
                None))
        .map_err(|e| IndexServerError::ServerLoopError(e))
}

#[derive(Clone)]
struct ConnTransformer<VT,ET,KT,S> {
    version_transform: VT,
    encrypt_transform: ET,
    keepalive_transform: KT,
    spawner: S,
}

impl<VT,ET,KT,S> ConnTransformer<VT,ET,KT,S> 
where
    VT: FutTransform<Input=ConnPairVec, Output=ConnPairVec> + Clone + Send,
    ET: FutTransform<Input=(Option<PublicKey>, ConnPairVec),
                     Output=Option<(PublicKey, ConnPairVec)>> + Clone + Send,
    KT: FutTransform<Input=ConnPairVec, Output=ConnPairVec> + Clone + Send,
    S: Spawn + Clone + Send,
{
    pub fn new(version_transform: VT,
               encrypt_transform: ET,
               keepalive_transform: KT,
               spawner: S) -> Self {

        ConnTransformer {
            version_transform,
            encrypt_transform,
            keepalive_transform,
            spawner,
        }
    }

    fn version_enc_keepalive(&self, 
                             opt_public_key: Option<PublicKey>, 
                             conn_pair: ConnPairVec)
                    -> BoxFuture<'_, Option<(PublicKey, ConnPairVec)>> 
    {
        let mut c_version_transform = self.version_transform.clone();
        let mut c_encrypt_transform = self.encrypt_transform.clone();
        let mut c_keepalive_transform = self.keepalive_transform.clone();
        Box::pin(async move {
            let conn_pair = await!(c_version_transform.transform(conn_pair));
            let (public_key, conn_pair) =
                await!(c_encrypt_transform.transform((opt_public_key, conn_pair)))?;
            let conn_pair = await!(c_keepalive_transform.transform(conn_pair));
            Some((public_key, conn_pair))
        })
    }


    /// Transform a raw connection from a client into connection with the following layers:
    /// - Version prefix
    /// - Encryption
    /// - keepalives
    /// - Serialization
    pub fn incoming_index_client_conn_transform(&self, conn_pair: ConnPairVec)
                    -> BoxFuture<'_, Option<(PublicKey, ConnPair<IndexServerToClient, IndexClientToServer>)>> 
    {
        let mut c_self = self.clone();
        Box::pin(async move {
            let (public_key, (mut sender, mut receiver)) = await!(c_self.version_enc_keepalive(None, conn_pair))?;

            let (user_sender, mut from_user_sender) = mpsc::channel(0);
            let (mut to_user_receiver, user_receiver) = mpsc::channel(0);

            // Deserialize received data
            let _ = c_self.spawner.spawn(async move {
                while let Some(data) = await!(receiver.next()) {
                    let message = match deserialize_index_client_to_server(&data) {
                        Ok(message) => message,
                        Err(_) => return,
                    };
                    if let Err(_) = await!(to_user_receiver.send(message)) {
                        return;
                    }
                }
            });

            // Serialize sent data:
            let _ = c_self.spawner.spawn(async move {
                while let Some(message) = await!(from_user_sender.next()) {
                    let data = serialize_index_server_to_client(&message);
                    if let Err(_) = await!(sender.send(data)) {
                        return;
                    }
                }
            });

            Some((public_key, (user_sender, user_receiver)))
        })
    }

    pub fn incoming_index_server_conn_transform(&self, conn_pair: ConnPairVec)
                    -> BoxFuture<'_, Option<(PublicKey, ConnPair<IndexServerToServer, IndexServerToServer>)>> 
    {
        let mut c_self = self.clone();
        Box::pin(async move {
            let (public_key, (mut sender, mut receiver)) = await!(c_self.version_enc_keepalive(None, conn_pair))?;

            let (user_sender, mut from_user_sender) = mpsc::channel(0);
            let (mut to_user_receiver, user_receiver) = mpsc::channel(0);

            // Deserialize received data
            let _ = c_self.spawner.spawn(async move {
                while let Some(data) = await!(receiver.next()) {
                    let message = match deserialize_index_server_to_server(&data) {
                        Ok(message) => message,
                        Err(_) => return,
                    };
                    if let Err(_) = await!(to_user_receiver.send(message)) {
                        return;
                    }
                }
            });

            // Serialize sent data:
            let _ = c_self.spawner.spawn(async move {
                while let Some(message) = await!(from_user_sender.next()) {
                    let data = serialize_index_server_to_server(&message);
                    if let Err(_) = await!(sender.send(data)) {
                        return;
                    }
                }
            });

            Some((public_key, (user_sender, user_receiver)))
        })
    }

    pub fn outgoing_index_server_conn_transform(&self, public_key: PublicKey, conn_pair: ConnPairVec)
                    -> BoxFuture<'_, Option<ConnPair<IndexServerToServer, IndexServerToServer>>> 
    {
        let mut c_self = self.clone();
        Box::pin(async move {
            let (_public_key, (mut sender, mut receiver)) = await!(c_self.version_enc_keepalive(Some(public_key), conn_pair))?;

            let (user_sender, mut from_user_sender) = mpsc::channel(0);
            let (mut to_user_receiver, user_receiver) = mpsc::channel(0);

            // Deserialize received data
            let _ = c_self.spawner.spawn(async move {
                while let Some(data) = await!(receiver.next()) {
                    let message = match deserialize_index_server_to_server(&data) {
                        Ok(message) => message,
                        Err(_) => return,
                    };
                    if let Err(_) = await!(to_user_receiver.send(message)) {
                        return;
                    }
                }
            });

            // Serialize sent data:
            let _ = c_self.spawner.spawn(async move {
                while let Some(message) = await!(from_user_sender.next()) {
                    let data = serialize_index_server_to_server(&message);
                    if let Err(_) = await!(sender.send(data)) {
                        return;
                    }
                }
            });

            Some((user_sender, user_receiver))
        })
    }
}

#[derive(Debug)]
pub enum NetIndexServerError {
    IndexServerError(IndexServerError),
    RequestPublicKeyError,
    SpawnError,
}


pub async fn net_index_server<A,ICC,ISC,SC,R,S>(incoming_client_raw_conns: ICC,
                    incoming_server_raw_conns: ISC,
                    raw_server_net_connector: SC,
                    identity_client: IdentityClient,
                    timer_client: TimerClient,
                    rng: R,
                    trusted_servers: HashMap<PublicKey, A>,
                    max_concurrent_encrypt: usize,
                    backoff_ticks: usize,
                    mut spawner: S) -> Result<(), NetIndexServerError> 
where
    A: Clone + Send + Debug + 'static,
    SC: FutTransform<Input=A,Output=Option<ConnPairVec>> + Clone + Send + 'static,
    ICC: Stream<Item=ConnPairVec> + Unpin + Send + 'static,
    ISC: Stream<Item=ConnPairVec> + Unpin + Send + 'static,
    R: CryptoRandom + Clone + 'static,
    S: Spawn + Clone + Send + Sync + 'static,
{

    let local_public_key = await!(identity_client.request_public_key())
        .map_err(|_| NetIndexServerError::RequestPublicKeyError)?;

    let version_transform = VersionPrefix::new(PROTOCOL_VERSION,
                                               spawner.clone());
    let encrypt_transform = SecureChannel::new(
        identity_client,
        rng.clone(),
        timer_client.clone(),
        TICKS_TO_REKEY,
        spawner.clone());

    let keepalive_transform = KeepAliveChannel::new(
        timer_client.clone(),
        KEEPALIVE_TICKS,
        spawner.clone());

    let conn_transformer = ConnTransformer::new(version_transform,
                         encrypt_transform,
                         keepalive_transform,
                         spawner.clone());

    // Transform incoming client connections:
    let c_conn_transformer = conn_transformer.clone();
    let incoming_client_transform = FuncFutTransform::new(move |raw_conn| {
        let c_conn_transformer = c_conn_transformer.clone();
        Box::pin(async move {
            await!(c_conn_transformer.incoming_index_client_conn_transform(raw_conn))
        })
    });
    let (client_conns_sender, incoming_client_conns) = mpsc::channel(0);
    let pool_fut = transform_pool_loop(incoming_client_raw_conns,
                        client_conns_sender,
                        incoming_client_transform,
                        max_concurrent_encrypt,
                        spawner.clone())
        .map_err(|e| error!("client incoming transform_pool_loop() error: {:?}", e))
        .map(|_| ());
    spawner.spawn(pool_fut)
        .map_err(|_| NetIndexServerError::SpawnError)?;

    // Transform incoming server connections:
    let c_conn_transformer = conn_transformer.clone();
    let incoming_server_transform = FuncFutTransform::new(move |raw_conn| {
        let c_conn_transformer = c_conn_transformer.clone();
        Box::pin(async move {
            await!(c_conn_transformer.incoming_index_server_conn_transform(raw_conn))
        })
    });
    let (server_conns_sender, incoming_server_conns) = mpsc::channel(0);
    let pool_fut = transform_pool_loop(incoming_server_raw_conns,
                        server_conns_sender,
                        incoming_server_transform,
                        max_concurrent_encrypt,
                        spawner.clone())
        .map_err(|e| error!("server incoming transform_pool_loop() error: {:?}", e))
        .map(|_| ());
    spawner.spawn(pool_fut)
        .map_err(|_| NetIndexServerError::SpawnError)?;

    // Apply transform to create server connector:
    let c_conn_transformer = conn_transformer.clone();
    let server_connector = FuncFutTransform::new(move |(public_key, net_address)| {
        let mut c_raw_server_net_connector = raw_server_net_connector.clone();
        let c_conn_transformer = c_conn_transformer.clone();
        Box::pin(async move {
            let raw_conn = await!(c_raw_server_net_connector.transform(net_address))?;
            await!(c_conn_transformer.outgoing_index_server_conn_transform(public_key, raw_conn))
        })
    });

    await!(index_server(local_public_key,
                   trusted_servers,
                   incoming_server_conns,
                   incoming_client_conns,
                   server_connector,
                   timer_client,
                   INDEX_NODE_TIMEOUT_TICKS,
                   backoff_ticks,
                   rng,
                   spawner.clone()))
        .map_err(|e| NetIndexServerError::IndexServerError(e))
}
