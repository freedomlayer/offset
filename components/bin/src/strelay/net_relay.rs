use std::marker::Unpin;

use futures::channel::mpsc;
use futures::task::{Spawn, SpawnExt};
use futures::{FutureExt, Stream, StreamExt, TryFutureExt};

use derive_more::*;

use common::conn::{BoxFuture, ConnPairVec, FutTransform};
use common::transform_pool::transform_pool_loop;

use proto::consts::{CONN_TIMEOUT_TICKS, KEEPALIVE_TICKS, PROTOCOL_VERSION, TICKS_TO_REKEY};
use proto::crypto::PublicKey;

use crypto::rand::CryptoRandom;

use identity::IdentityClient;
use timer::TimerClient;

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
struct AnonSecureChannel<ET> {
    encrypt_transform: ET,
}

impl<ET> AnonSecureChannel<ET> {
    pub fn new(encrypt_transform: ET) -> Self {
        AnonSecureChannel { encrypt_transform }
    }
}

impl<ET> FutTransform for AnonSecureChannel<ET>
where
    ET: FutTransform<
        Input = (Option<PublicKey>, ConnPairVec),
        Output = Option<(PublicKey, ConnPairVec)>,
    >,
{
    type Input = ConnPairVec;
    type Output = Option<(PublicKey, ConnPairVec)>;

    fn transform(&mut self, conn_pair: Self::Input) -> BoxFuture<'_, Self::Output> {
        self.encrypt_transform.transform((None, conn_pair))
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
    let version_transform = VersionPrefix::new(PROTOCOL_VERSION, spawner.clone());

    let encrypt_transform = SecureChannel::new(
        identity_client,
        rng,
        timer_client.clone(),
        TICKS_TO_REKEY,
        spawner.clone(),
    );

    // TODO: How to get rid of Box::pin() here?
    let incoming_ver_conns = Box::pin(incoming_raw_conns.then(move |raw_conn| {
        // TODO: A more efficient way to do this?
        // We seem to have to clone version_transform for every connection
        // to make the borrow checker happy.
        let mut c_version_transform = version_transform.clone();
        async move { c_version_transform.transform(raw_conn).await }
    }));

    let (enc_conns_sender, incoming_enc_conns) = mpsc::channel::<(PublicKey, ConnPairVec)>(0);

    let enc_pool_fut = transform_pool_loop(
        incoming_ver_conns,
        enc_conns_sender,
        AnonSecureChannel::new(encrypt_transform),
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

