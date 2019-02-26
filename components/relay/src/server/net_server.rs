use std::marker::Unpin;

use futures::{FutureExt, TryFutureExt, Stream, StreamExt};
use futures::task::{Spawn, SpawnExt};
use futures::channel::mpsc;

use common::conn::{ConnPairVec, FutTransform, BoxFuture};
use common::transform_pool::transform_pool_loop;

use proto::consts::{PROTOCOL_VERSION, TICKS_TO_REKEY, KEEPALIVE_TICKS, CONN_TIMEOUT_TICKS};

use crypto::identity::PublicKey;
use crypto::crypto_rand::CryptoRandom;

use timer::TimerClient;
use keepalive::KeepAliveChannel;
use identity::IdentityClient;

use version::VersionPrefix;
use secure_channel::SecureChannel;

use super::server::relay_server_loop;
pub use super::server::RelayServerError;
use super::conn_processor::conn_processor;

/// A relay server loop. Incoming connections should contain both (sender, receiver) and a
/// public_key of the remote side (Should be obtained after authentication).
///
/// `conn_timeout_ticks` is the amount of time we are willing to wait for a connection to identify
/// its purpose.
/// `keepalive_ticks` is the amount of time we are willing to let the remote side to be idle before
/// we disconnect. It is also used to timeout open half tunnels that were not claimed.
async fn relay_server<IC,S>(incoming_conns: IC,
                                timer_client: TimerClient,
                                conn_timeout_ticks: usize,
                                keepalive_ticks: usize,
                                spawner: S) -> Result<(), RelayServerError>
where
    S: Spawn + Clone + Send + 'static,
    IC: Stream<Item=(PublicKey, ConnPairVec)> + Unpin + Send + 'static,
{

    let keepalive_transform = KeepAliveChannel::new(
        timer_client.clone(),
        keepalive_ticks,
        spawner.clone());

    // TODO: How to get rid of the Box::pin here?
    let processed_conns = Box::pin(conn_processor(incoming_conns,
                   keepalive_transform,
                   timer_client.clone(),
                   conn_timeout_ticks));

    await!(relay_server_loop(timer_client,
                              processed_conns,
                              keepalive_ticks,
                              spawner))

}


#[derive(Debug)]
pub enum NetRelayServerError {
    RelayServerError(RelayServerError),
    SpawnError,
}

impl From<RelayServerError> for NetRelayServerError {
    fn from(e: RelayServerError) -> Self {
        NetRelayServerError::RelayServerError(e)
    }
}

/// Start a secure channel without knowing the identity of the remote
/// side ahead of time.
#[derive(Clone)]
struct AnonSecureChannel<ET> {
    encrypt_transform: ET,
}

impl<ET> AnonSecureChannel<ET> {
    pub fn new(encrypt_transform: ET) -> Self {
        AnonSecureChannel {
            encrypt_transform,
        }
    }
}

impl<ET> FutTransform for AnonSecureChannel<ET> 
where
    ET: FutTransform<Input=(Option<PublicKey>, ConnPairVec),
                     Output=Option<(PublicKey, ConnPairVec)>>,
{
    type Input = ConnPairVec;
    type Output = Option<(PublicKey, ConnPairVec)>;

    fn transform(&mut self, conn_pair: Self::Input)
        -> BoxFuture<'_, Self::Output> {

        self.encrypt_transform.transform((None, conn_pair))
    }
}

#[allow(unused)]
pub async fn net_relay_server<IRC,R,S>(incoming_raw_conns: IRC,
                             identity_client: IdentityClient,
                             timer_client: TimerClient,
                             rng: R,
                             max_concurrent_encrypt: usize,
                             mut spawner: S) -> Result<(), NetRelayServerError> 
where
    IRC: Stream<Item=ConnPairVec> + Unpin + Send + 'static,
    R: CryptoRandom + Clone + 'static,
    S: Spawn + Clone + Send + Sync + 'static,
{

    let version_transform = VersionPrefix::new(PROTOCOL_VERSION,
                                               spawner.clone());

    let encrypt_transform = SecureChannel::new(
        identity_client,
        rng,
        timer_client.clone(),
        TICKS_TO_REKEY,
        spawner.clone());


    // TODO; How to get rid of Box::pin() here?
    let incoming_ver_conns = Box::pin(incoming_raw_conns
        .then(move |raw_conn| {
            // TODO: A more efficient way to do this?
            // We seem to have to clone version_transform for every connection
            // to make the borrow checker happy.
            let mut c_version_transform = version_transform.clone();
            async move {
                await!(c_version_transform.transform(raw_conn))
            }
        }));

    let (enc_conns_sender, incoming_enc_conns) = mpsc::channel::<(PublicKey, ConnPairVec)>(0);

    let enc_pool_fut = transform_pool_loop(
            incoming_ver_conns,
            enc_conns_sender,
            AnonSecureChannel::new(encrypt_transform),
            max_concurrent_encrypt,
            spawner.clone())
        .map_err(|e| error!("transform_pool_loop() error: {:?}", e))
        .map(|_| ());

    spawner.spawn(enc_pool_fut)
        .map_err(|_| NetRelayServerError::SpawnError)?;

    await!(relay_server(incoming_enc_conns,
                timer_client,
                CONN_TIMEOUT_TICKS,
                KEEPALIVE_TICKS,
                spawner.clone()))?;
    Ok(())
}
