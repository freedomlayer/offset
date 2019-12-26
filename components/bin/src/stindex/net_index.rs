use std::collections::HashMap;
use std::fmt::Debug;
use std::marker::Unpin;

use futures::channel::mpsc;
use futures::task::{Spawn, SpawnExt};
use futures::{FutureExt, SinkExt, Stream, StreamExt, TryFutureExt};

use common::conn::{BoxFuture, ConnPair, ConnPairVec, FuncFutTransform, FutTransform};
use common::transform_pool::transform_pool_loop;

use proto::consts::{INDEX_NODE_TIMEOUT_TICKS};
use proto::crypto::PublicKey;
use proto::index_server::messages::{
    IndexClientToServer, IndexServerToClient, IndexServerToServer,
};

use proto::proto_ser::{ProtoDeserialize, ProtoSerialize};

use timer::TimerClient;

use crypto::rand::CryptoRandom;

use identity::IdentityClient;

use connection::create_version_encrypt_keepalive;

use index_server::{index_server, IndexServerError};

#[derive(Clone)]
struct ConnTransformer<CT, S> {
    conn_transform: CT,
    spawner: S,
}

impl<CT, S> ConnTransformer<CT, S>
where
    CT: FutTransform<
            Input = (Option<PublicKey>, ConnPairVec),
            Output = Option<(PublicKey, ConnPairVec)>,
        > + Clone
        + Send,
    S: Spawn + Clone + Send,
{
    pub fn new(
        conn_transform: CT,
        spawner: S,
    ) -> Self {
        ConnTransformer {
            conn_transform,
            spawner,
        }
    }

    fn version_enc_keepalive(
        &mut self,
        opt_public_key: Option<PublicKey>,
        conn_pair: ConnPairVec,
    ) -> BoxFuture<'_, Option<(PublicKey, ConnPairVec)>> {
        let mut c_conn_transform = self.conn_transform.clone();
        Box::pin(async move {
            c_conn_transform
                .transform((opt_public_key, conn_pair))
                .await
        })
    }

    /// Transform a raw connection from a client into connection with the following layers:
    /// - Version prefix
    /// - Encryption
    /// - keepalives
    /// - Serialization
    pub fn incoming_index_client_conn_transform(
        &mut self,
        conn_pair: ConnPairVec,
    ) -> BoxFuture<
        '_,
        Option<(
            PublicKey,
            ConnPair<IndexServerToClient, IndexClientToServer>,
        )>,
    > {
        let mut c_self = self.clone();
        Box::pin(async move {
            let (public_key, conn_pair) = c_self.version_enc_keepalive(None, conn_pair).await?;

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
        &mut self,
        conn_pair: ConnPairVec,
    ) -> BoxFuture<
        '_,
        Option<(
            PublicKey,
            ConnPair<IndexServerToServer, IndexServerToServer>,
        )>,
    > {
        let mut c_self = self.clone();
        Box::pin(async move {
            let (public_key, conn_pair) = c_self.version_enc_keepalive(None, conn_pair).await?;

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
        &mut self,
        public_key: PublicKey,
        conn_pair: ConnPairVec,
    ) -> BoxFuture<'_, Option<ConnPair<IndexServerToServer, IndexServerToServer>>> {
        let mut c_self = self.clone();
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


    let conn_transform = create_version_encrypt_keepalive(
        timer_client.clone(),
        identity_client.clone(),
        rng.clone(),
        spawner.clone());

    let conn_transformer = ConnTransformer::new(
        conn_transform,
        spawner.clone(),
    );

    // Transform incoming client connections:
    let c_conn_transformer = conn_transformer.clone();
    let incoming_client_transform = FuncFutTransform::new(move |raw_conn| {
        let mut c_conn_transformer = c_conn_transformer.clone();
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
        let mut c_conn_transformer = c_conn_transformer.clone();
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
        let mut c_conn_transformer = c_conn_transformer.clone();
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
