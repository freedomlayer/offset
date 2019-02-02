use std::fmt::Debug;
use std::marker::Unpin;

use futures::task::Spawn;
use futures::Stream;

use common::conn::FutTransform;

use crypto::identity::{PublicKey, compare_public_key};
use crypto::crypto_rand::CryptoRandom;

use crate::server::{server_loop, ServerLoopError};
pub use crate::server::{ServerConn, ClientConn, IndexServerConfig};

use crate::verifier::simple_verifier::SimpleVerifier;
use crate::graph::graph_service::create_graph_service;
use crate::graph::simple_capacity_graph::SimpleCapacityGraph;


#[derive(Debug)]
pub enum IndexServerError {
    RequestTimerStreamError,
    CreateGraphServiceError,
    ServerLoopError(ServerLoopError),
}


/// Run an index server
/// Will keep running until an error occurs.
pub async fn index_server<A,IS,IC,SC,TS,R,S>(index_server_config: IndexServerConfig<A>,
                           incoming_server_connections: IS,
                           incoming_client_connections: IC,
                           server_connector: SC,
                           timer_stream: TS,
                           ticks_to_live: usize,
                           rng: R,
                           spawner: S) 
                                -> Result<(), IndexServerError>
where
    A: Debug + Send + Clone + 'static,
    IS: Stream<Item=(PublicKey, ServerConn)> + Unpin,
    IC: Stream<Item=(PublicKey, ClientConn)> + Unpin,
    SC: FutTransform<Input=(PublicKey, A), Output=ServerConn> + Clone + Send + 'static,
    TS: Stream + Unpin,
    R: CryptoRandom,
    S: Spawn + Clone + Send,
{

    let verifier = SimpleVerifier::new(ticks_to_live, rng);

    let capacity_graph = SimpleCapacityGraph::new();
    let graph_client = create_graph_service(capacity_graph, spawner.clone())
        .map_err(|_| IndexServerError::CreateGraphServiceError)?;

    await!(server_loop(index_server_config,
                incoming_server_connections,
                incoming_client_connections,
                server_connector,
                graph_client,
                compare_public_key,
                verifier,
                timer_stream,
                spawner,
                None))
        .map_err(|e| IndexServerError::ServerLoopError(e))
}
