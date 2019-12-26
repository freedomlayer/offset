use std::marker::Unpin;

use futures::channel::mpsc;
use futures::task::{Spawn, SpawnExt};
use futures::{FutureExt, Stream, TryFutureExt};

use derive_more::*;

use common::conn::{BoxFuture, ConnPairVec, FutTransform};
use common::transform_pool::transform_pool_loop;

use proto::consts::{CONN_TIMEOUT_TICKS, KEEPALIVE_TICKS};
use proto::crypto::PublicKey;

use crypto::rand::CryptoRandom;

use identity::IdentityClient;
use timer::TimerClient;

use connection::create_version_encrypt_keepalive;

use relay::{relay_server, RelayServerError};

#[derive(Debug, From)]
pub enum NetRelayServerError {
    RelayServerError(RelayServerError),
    SpawnError,
}

/// Start a secure channel without knowing the identity of the remote
/// side ahead of time.
#[derive(Clone)]
struct AnonSecureChannel<R, S> {
    timer_client: TimerClient,
    identity_client: IdentityClient,
    rng: R,
    spawner: S,
}

impl<R, S> AnonSecureChannel<R, S> {
    pub fn new(
        timer_client: TimerClient,
        identity_client: IdentityClient,
        rng: R,
        spawner: S,
    ) -> Self {
        Self {
            timer_client,
            identity_client,
            rng,
            spawner,
        }
    }
}

impl<R, S> FutTransform for AnonSecureChannel<R, S>
where
    R: CryptoRandom + Clone + 'static,
    S: Spawn + Clone + Send + 'static,
{
    type Input = ConnPairVec;
    type Output = Option<(PublicKey, ConnPairVec)>;

    fn transform(&mut self, conn_pair: Self::Input) -> BoxFuture<'_, Self::Output> {

        let mut conn_transform = create_version_encrypt_keepalive(
            self.timer_client.clone(),
            self.identity_client.clone(),
            self.rng.clone(),
            self.spawner.clone());

        Box::pin(async move {
            let (public_key, conn_pair) = conn_transform.transform((None, conn_pair)).await?;
            Some((public_key, conn_pair))
        })
    }
}

pub async fn net_relay_server<IRC, R, S>(
    incoming_raw_conns: IRC,
    identity_client: IdentityClient,
    timer_client: TimerClient,
    rng: R,
    max_concurrent_encrypt: usize,
    spawner: S,
) -> Result<(), NetRelayServerError>
where
    IRC: Stream<Item = ConnPairVec> + Unpin + Send + 'static,
    R: CryptoRandom + Clone + 'static,
    S: Spawn + Clone + Send + Sync + 'static,
{
    let (enc_conns_sender, incoming_enc_conns) = mpsc::channel::<(PublicKey, ConnPairVec)>(0);

    let transform = AnonSecureChannel::new(
        timer_client.clone(),
        identity_client.clone(),
        rng,
        spawner.clone(),
    );

    let enc_pool_fut = transform_pool_loop(
        incoming_raw_conns,
        enc_conns_sender,
        transform,
        max_concurrent_encrypt,
        spawner.clone(),
    )
    .map_err(|e| error!("transform_pool_loop() error: {:?}", e))
    .map(|_| ());

    spawner
        .spawn(enc_pool_fut)
        .map_err(|_| NetRelayServerError::SpawnError)?;

    relay_server(
        incoming_enc_conns,
        timer_client,
        CONN_TIMEOUT_TICKS,
        KEEPALIVE_TICKS,
        spawner.clone(),
    )
    .await?;
    Ok(())
}
