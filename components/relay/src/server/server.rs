use std::marker::Unpin;

use futures::task::{Spawn};
use futures::Stream;


use common::conn::{ConnPairVec};

use proto::crypto::PublicKey;

use timer::TimerClient;

use crate::server::conn_processor::conn_processor;
use crate::server::server_loop::{relay_server_loop, RelayServerError};

/// A relay server loop. Incoming connections should contain both (sender, receiver) and a
/// public_key of the remote side (Should be obtained after authentication).
///
/// `conn_timeout_ticks` is the amount of time we are willing to wait for a connection to identify
/// its purpose.
pub async fn relay_server<IC, S>(
    incoming_conns: IC,
    timer_client: TimerClient,
    conn_timeout_ticks: usize,
    half_tunnel_ticks: usize,
    spawner: S,
) -> Result<(), RelayServerError>
where
    S: Spawn + Clone + Send + 'static,
    IC: Stream<Item = (PublicKey, ConnPairVec)> + Unpin + Send + 'static,
{

    // TODO: How to get rid of the Box::pin here?
    let processed_conns = Box::pin(conn_processor(
        incoming_conns,
        timer_client.clone(),
        conn_timeout_ticks,
    ));

    relay_server_loop(timer_client, processed_conns, half_tunnel_ticks, spawner).await
}

