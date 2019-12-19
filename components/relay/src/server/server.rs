use std::marker::Unpin;

use futures::task::{Spawn};
use futures::Stream;


use common::conn::{ConnPairVec};

use proto::crypto::PublicKey;

use timer::TimerClient;
use keepalive::KeepAliveChannel;

use crate::server::conn_processor::conn_processor;
use crate::server::server_loop::{relay_server_loop, RelayServerError};

/// A relay server loop. Incoming connections should contain both (sender, receiver) and a
/// public_key of the remote side (Should be obtained after authentication).
///
/// `conn_timeout_ticks` is the amount of time we are willing to wait for a connection to identify
/// its purpose.
/// `keepalive_ticks` is the amount of time we are willing to let the remote side to be idle before
/// we disconnect. It is also used to timeout open half tunnels that were not claimed.
pub async fn relay_server<IC, S>(
    incoming_conns: IC,
    timer_client: TimerClient,
    conn_timeout_ticks: usize,
    keepalive_ticks: usize,
    spawner: S,
) -> Result<(), RelayServerError>
where
    S: Spawn + Clone + Send + 'static,
    IC: Stream<Item = (PublicKey, ConnPairVec)> + Unpin + Send + 'static,
{
    // TODO: Eliminate keepalive_transform from this code.
    let keepalive_transform =
        KeepAliveChannel::new(timer_client.clone(), keepalive_ticks, spawner.clone());

    // TODO: How to get rid of the Box::pin here?
    let processed_conns = Box::pin(conn_processor(
        incoming_conns,
        keepalive_transform,
        timer_client.clone(),
        conn_timeout_ticks,
    ));

    // TODO:
    // This is a hack to avoid having the relay client
    // disconnect from the relay server too early because of the underlying keepalive.
    // We should find a more elegant way to solve this problem.
    let half_tunnel_ticks = keepalive_ticks / 2;
    assert!(half_tunnel_ticks < keepalive_ticks);
    assert!(half_tunnel_ticks > 0);

    relay_server_loop(timer_client, processed_conns, half_tunnel_ticks, spawner).await
}

