use std::collections::HashMap;
use std::fmt::Debug;
use std::marker::Unpin;

use futures::channel::mpsc;
use futures::task::{Spawn, SpawnExt};
use futures::{FutureExt, SinkExt, Stream, StreamExt, TryFutureExt};

use common::conn::{BoxFuture, ConnPair, ConnPairVec, FuncFutTransform, FutTransform};
use common::transform_pool::transform_pool_loop;

use proto::consts::{INDEX_NODE_TIMEOUT_TICKS, KEEPALIVE_TICKS, PROTOCOL_VERSION, TICKS_TO_REKEY};
use proto::crypto::PublicKey;
use proto::index_server::messages::{
    IndexClientToServer, IndexServerToClient, IndexServerToServer,
};

use proto::proto_ser::{ProtoDeserialize, ProtoSerialize};

/*
use proto::index_server::serialize::{
    deserialize_index_client_to_server, deserialize_index_server_to_server,
    serialize_index_server_to_client, serialize_index_server_to_server,
};
*/

use timer::TimerClient;

use crypto::identity::compare_public_key;
use crypto::rand::CryptoRandom;

use identity::IdentityClient;
use keepalive::KeepAliveChannel;
use secure_channel::SecureChannel;
use version::VersionPrefix;

use crate::server::{server_loop, ServerLoopError};
pub use crate::server::{ClientConn, ServerConn};

use crate::backoff_connector::BackoffConnector;
use crate::graph::graph_service::create_graph_service;
use crate::graph::simple_capacity_graph::SimpleCapacityGraph;
use crate::verifier::simple_verifier::SimpleVerifier;

#[derive(Debug)]
pub enum IndexServerError {
    RequestTimerStreamError,
    CreateGraphServiceError,
    ServerLoopError(ServerLoopError),
}

/// Run an index server
/// Will keep running until an error occurs.
async fn index_server<A, IS, IC, SC, R, GS, S>(
    local_public_key: PublicKey,
    trusted_servers: HashMap<PublicKey, A>,
    incoming_server_connections: IS,
    incoming_client_connections: IC,
    server_connector: SC,
    mut timer_client: TimerClient,
    ticks_to_live: usize,
    backoff_ticks: usize,
    rng: R,
    graph_service_spawner: GS,
    spawner: S,
) -> Result<(), IndexServerError>
where
    A: Debug + Send + Sync + Clone + 'static,
    IS: Stream<Item = (PublicKey, ServerConn)> + Unpin + Send,
    IC: Stream<Item = (PublicKey, ClientConn)> + Unpin + Send,
    SC: FutTransform<Input = (PublicKey, A), Output = Option<ServerConn>> + Clone + Send + 'static,
    R: CryptoRandom,
    S: Spawn + Clone + Send,
    GS: Spawn + Send + 'static,
{
    let verifier = SimpleVerifier::new(ticks_to_live, rng);

    let graph_client = create_graph_service::<_, _, _, _, SimpleCapacityGraph<_, _>, _, _>(
        graph_service_spawner,
        spawner.clone(),
    )
    .map_err(|_| IndexServerError::CreateGraphServiceError)?;

    let timer_stream = timer_client
        .request_timer_stream()
        .await
        .map_err(|_| IndexServerError::RequestTimerStreamError)?;

    let backoff_connector = BackoffConnector::new(server_connector, timer_client, backoff_ticks);

    server_loop(
        local_public_key,
        trusted_servers,
        incoming_server_connections,
        incoming_client_connections,
        backoff_connector,
        graph_client,
        compare_public_key,
        verifier,
        timer_stream,
        spawner,
        None,
    )
    .await
    .map_err(IndexServerError::ServerLoopError)
}

#[derive(Clone)]
struct ConnTransformer<VT, ET, KT, S> {
    version_transform: VT,
    encrypt_transform: ET,
    keepalive_transform: KT,
    spawner: S,
}

impl<VT, ET, KT, S> ConnTransformer<VT, ET, KT, S>
where
    VT: FutTransform<Input = ConnPairVec, Output = ConnPairVec> + Clone + Send + Sync,
    ET: FutTransform<
            Input = (Option<PublicKey>, ConnPairVec),
            Output = Option<(PublicKey, ConnPairVec)>,
        > + Clone
        + Send
        + Sync,
    KT: FutTransform<Input = ConnPairVec, Output = ConnPairVec> + Clone + Send + Sync,
    S: Spawn + Clone + Send + Sync,
{
    pub fn new(
        version_transform: VT,
        encrypt_transform: ET,
        keepalive_transform: KT,
        spawner: S,
    ) -> Self {
        ConnTransformer {
            version_transform,
            encrypt_transform,
            keepalive_transform,
            spawner,
        }
    }

    fn version_enc_keepalive(
        &self,
        opt_public_key: Option<PublicKey>,
        conn_pair: ConnPairVec,
    ) -> BoxFuture<'_, Option<(PublicKey, ConnPairVec)>> {
        let mut c_version_transform = self.version_transform.clone();
        let mut c_encrypt_transform = self.encrypt_transform.clone();
        let mut c_keepalive_transform = self.keepalive_transform.clone();
        Box::pin(async move {
            let conn_pair = c_version_transform.transform(conn_pair).await;
            let (public_key, conn_pair) = c_encrypt_transform
                .transform((opt_public_key, conn_pair))
                .await?;
            let conn_pair = c_keepalive_transform.transform(conn_pair).await;
            Some((public_key, conn_pair))
        })
    }

    /// Transform a raw connection from a client into connection with the following layers:
    /// - Version prefix
    /// - Encryption
    /// - keepalives
    /// - Serialization
    pub fn incoming_index_client_conn_transform(
        &self,
        conn_pair: ConnPairVec,
    ) -> BoxFuture<
        '_,
        Option<(
            PublicKey,
            ConnPair<IndexServerToClient, IndexClientToServer>,
        )>,
    > {
        let c_self = self.clone();
        Box::pin(async move {
            let (public_key, conn_pair) =
                c_self.version_enc_keepalive(None, conn_pair).await?;
            
            let (mut sender, mut receiver) = conn_pair.split();

            let (user_sender, mut from_user_sender) = mpsc::channel::<IndexServerToClient>(0);
            let (mut to_user_receiver, user_receiver) = mpsc::channel(0);

            // Deserialize received data
            let _ = c_self.spawner.spawn(async move {
                while let Some(data) = receiver.next().await {
                    let message = match IndexClientToServer::proto_deserialize(&data) {
                        Ok(message) => message,
                        Err(_) => {
                            error!("Error deserializing index_client_to_server");
                            return;
                        }
                    };
                    if to_user_receiver.send(message).await.is_err() {
                        return;
                    }
                }
            });

            // Serialize sent data:
            let _ = c_self.spawner.spawn(async move {
                while let Some(message) = from_user_sender.next().await {
                    // let data = serialize_index_server_to_client(&message);
                    let data = message.proto_serialize();
                    if sender.send(data).await.is_err() {
                        return;
                    }
                }
            });

            Some((public_key, ConnPair::from_raw(user_sender, user_receiver)))
        })
    }

    pub fn incoming_index_server_conn_transform(
        &self,
        conn_pair: ConnPairVec,
    ) -> BoxFuture<
        '_,
        Option<(
            PublicKey,
            ConnPair<IndexServerToServer, IndexServerToServer>,
        )>,
    > {
        let c_self = self.clone();
        Box::pin(async move {
            let (public_key, conn_pair) =
                c_self.version_enc_keepalive(None, conn_pair).await?;

            let (mut sender, mut receiver) = conn_pair.split();

            let (user_sender, mut from_user_sender) = mpsc::channel::<IndexServerToServer>(0);
            let (mut to_user_receiver, user_receiver) = mpsc::channel(0);

            // Deserialize received data
            let _ = c_self.spawner.spawn(async move {
                while let Some(data) = receiver.next().await {
                    let message = match IndexServerToServer::proto_deserialize(&data) {
                        Ok(message) => message,
                        Err(_) => {
                            error!("Error deserializing index_server_to_server");
                            return;
                        }
                    };
                    if to_user_receiver.send(message).await.is_err() {
                        return;
                    }
                }
            });

            // Serialize sent data:
            let _ = c_self.spawner.spawn(async move {
                while let Some(message) = from_user_sender.next().await {
                    // let data = serialize_index_server_to_server(&message);
                    let data = message.proto_serialize();
                    if sender.send(data).await.is_err() {
                        return;
                    }
                }
            });

            Some((public_key, ConnPair::from_raw(user_sender, user_receiver)))
        })
    }

    pub fn outgoing_index_server_conn_transform(
        &self,
        public_key: PublicKey,
        conn_pair: ConnPairVec,
    ) -> BoxFuture<'_, Option<ConnPair<IndexServerToServer, IndexServerToServer>>> {
        let c_self = self.clone();
        Box::pin(async move {
            let (_public_key, conn_pair) = c_self
                .version_enc_keepalive(Some(public_key), conn_pair)
                .await?;

            let (mut sender, mut receiver) = conn_pair.split();

            let (user_sender, mut from_user_sender) = mpsc::channel::<IndexServerToServer>(0);
            let (mut to_user_receiver, user_receiver) = mpsc::channel(0);

            // Deserialize received data
            let _ = c_self.spawner.spawn(async move {
                while let Some(data) = receiver.next().await {
                    let message = match IndexServerToServer::proto_deserialize(&data) {
                        Ok(message) => message,
                        Err(_) => {
                            error!("Error deserializing index_server_to_server");
                            return;
                        }
                    };
                    if to_user_receiver.send(message).await.is_err() {
                        return;
                    }
                }
            });

            // Serialize sent data:
            let _ = c_self.spawner.spawn(async move {
                while let Some(message) = from_user_sender.next().await {
                    // let data = serialize_index_server_to_server(&message);
                    let data = message.proto_serialize();
                    if sender.send(data).await.is_err() {
                        return;
                    }
                }
            });

            Some(ConnPair::from_raw(user_sender, user_receiver))
        })
    }
}

#[derive(Debug)]
pub enum NetIndexServerError {
    IndexServerError(IndexServerError),
    RequestPublicKeyError,
    SpawnError,
}

pub async fn net_index_server<A, ICC, ISC, SC, R, GS, S>(
    incoming_client_raw_conns: ICC,
    incoming_server_raw_conns: ISC,
    raw_server_net_connector: SC,
    identity_client: IdentityClient,
    timer_client: TimerClient,
    rng: R,
    trusted_servers: HashMap<PublicKey, A>,
    max_concurrent_encrypt: usize,
    backoff_ticks: usize,
    graph_service_spawner: GS,
    spawner: S,
) -> Result<(), NetIndexServerError>
where
    A: Clone + Send + Sync + Debug + 'static,
    SC: FutTransform<Input = A, Output = Option<ConnPairVec>> + Clone + Send + 'static,
    ICC: Stream<Item = ConnPairVec> + Unpin + Send + 'static,
    ISC: Stream<Item = ConnPairVec> + Unpin + Send + 'static,
    R: CryptoRandom + Clone + 'static,
    GS: Spawn + Send + 'static,
    S: Spawn + Clone + Send + Sync + 'static,
{
    let local_public_key = identity_client
        .request_public_key()
        .await
        .map_err(|_| NetIndexServerError::RequestPublicKeyError)?;

    let version_transform = VersionPrefix::new(PROTOCOL_VERSION, spawner.clone());
    let encrypt_transform = SecureChannel::new(
        identity_client,
        rng.clone(),
        timer_client.clone(),
        TICKS_TO_REKEY,
        spawner.clone(),
    );

    let keepalive_transform =
        KeepAliveChannel::new(timer_client.clone(), KEEPALIVE_TICKS, spawner.clone());

    let conn_transformer = ConnTransformer::new(
        version_transform,
        encrypt_transform,
        keepalive_transform,
        spawner.clone(),
    );

    // Transform incoming client connections:
    let c_conn_transformer = conn_transformer.clone();
    let incoming_client_transform = FuncFutTransform::new(move |raw_conn| {
        let c_conn_transformer = c_conn_transformer.clone();
        Box::pin(async move {
            c_conn_transformer
                .incoming_index_client_conn_transform(raw_conn)
                .await
        })
    });
    let (client_conns_sender, incoming_client_conns) = mpsc::channel(0);
    let pool_fut = transform_pool_loop(
        incoming_client_raw_conns,
        client_conns_sender,
        incoming_client_transform,
        max_concurrent_encrypt,
        spawner.clone(),
    )
    .map_err(|e| error!("client incoming transform_pool_loop() error: {:?}", e))
    .map(|_| ());
    spawner
        .spawn(pool_fut)
        .map_err(|_| NetIndexServerError::SpawnError)?;

    // Transform incoming server connections:
    let c_conn_transformer = conn_transformer.clone();
    let incoming_server_transform = FuncFutTransform::new(move |raw_conn| {
        let c_conn_transformer = c_conn_transformer.clone();
        Box::pin(async move {
            c_conn_transformer
                .incoming_index_server_conn_transform(raw_conn)
                .await
        })
    });
    let (server_conns_sender, incoming_server_conns) = mpsc::channel(0);
    let pool_fut = transform_pool_loop(
        incoming_server_raw_conns,
        server_conns_sender,
        incoming_server_transform,
        max_concurrent_encrypt,
        spawner.clone(),
    )
    .map_err(|e| error!("server incoming transform_pool_loop() error: {:?}", e))
    .map(|_| ());
    spawner
        .spawn(pool_fut)
        .map_err(|_| NetIndexServerError::SpawnError)?;

    // Apply transform to create server connector:
    let c_conn_transformer = conn_transformer.clone();
    let server_connector = FuncFutTransform::new(move |(public_key, net_address)| {
        let mut c_raw_server_net_connector = raw_server_net_connector.clone();
        let c_conn_transformer = c_conn_transformer.clone();
        Box::pin(async move {
            let raw_conn = c_raw_server_net_connector.transform(net_address).await?;
            c_conn_transformer
                .outgoing_index_server_conn_transform(public_key, raw_conn)
                .await
        })
    });

    index_server(
        local_public_key,
        trusted_servers,
        incoming_server_conns,
        incoming_client_conns,
        server_connector,
        timer_client,
        INDEX_NODE_TIMEOUT_TICKS,
        backoff_ticks,
        rng,
        graph_service_spawner,
        spawner.clone(),
    )
    .await
    .map_err(NetIndexServerError::IndexServerError)
}
