use std::collections::HashMap;
use std::fmt::Debug;
use std::marker::Unpin;

use futures::task::Spawn;
use futures::Stream;

use common::conn::FutTransform;

use proto::crypto::PublicKey;

use timer::TimerClient;

use crypto::identity::compare_public_key;
use crypto::rand::CryptoRandom;

use crate::server_loop::{server_loop, ClientConn, ServerConn, ServerLoopError};

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
pub async fn index_server<A, IS, IC, SC, R, GS, S>(
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
