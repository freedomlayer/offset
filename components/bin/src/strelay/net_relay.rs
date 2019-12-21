use std::marker::Unpin;

use futures::channel::mpsc;
use futures::task::{Spawn, SpawnExt};
use futures::{FutureExt, Stream, TryFutureExt};

use derive_more::*;

use common::conn::{BoxFuture, ConnPairVec, FutTransform};
use common::transform_pool::transform_pool_loop;

use proto::consts::{CONN_TIMEOUT_TICKS, KEEPALIVE_TICKS, PROTOCOL_VERSION, TICKS_TO_REKEY};
use proto::crypto::PublicKey;

use crypto::rand::CryptoRandom;

use identity::IdentityClient;
use timer::TimerClient;

use keepalive::KeepAliveChannel;
use secure_channel::SecureChannel;
use version::VersionPrefix;

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
        let mut version_transform = VersionPrefix::new(PROTOCOL_VERSION, self.spawner.clone());
        let mut encrypt_transform = SecureChannel::new(
            self.identity_client.clone(),
            self.rng.clone(),
            self.timer_client.clone(),
            TICKS_TO_REKEY,
            self.spawner.clone(),
        );
        let mut keepalive_transform = KeepAliveChannel::new(
            self.timer_client.clone(),
            KEEPALIVE_TICKS,
            self.spawner.clone(),
        );

        Box::pin(async move {
            let conn_pair = version_transform.transform(conn_pair).await;
            let (public_key, conn_pair) = encrypt_transform.transform((None, conn_pair)).await?;
            let conn_pair = keepalive_transform.transform(conn_pair).await;
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
