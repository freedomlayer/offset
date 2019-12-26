use futures::task::{Spawn};

use common::conn::{ConnPairVec, FutTransform, FuncFutTransform};

use proto::consts::{KEEPALIVE_TICKS, TICKS_TO_REKEY};
use proto::crypto::PublicKey;

use crypto::rand::CryptoRandom;

use identity::IdentityClient;
use timer::TimerClient;

use keepalive::KeepAliveChannel;
use secure_channel::SecureChannel;

/// Create an encrypt-keepalive transformation:
/// Composes: Encryption * Keepalive
pub fn create_encrypt_keepalive<R, S>(
    timer_client: TimerClient,
    identity_client: IdentityClient,
    rng: R,
    spawner: S) -> impl FutTransform<Input = (Option<PublicKey>, ConnPairVec), Output = Option<(PublicKey, ConnPairVec)>> 
        + Clone
        + Send
        + 'static
where
    S: Spawn + Clone + Send + 'static,
    R: CryptoRandom + Clone + 'static,
{

    // Wrap the connection (Version * Encrypt * Keepalive):
    let encrypt_transform = SecureChannel::new(
        identity_client,
        rng,
        timer_client.clone(),
        TICKS_TO_REKEY,
        spawner.clone(),
    );
    let keepalive_transform =
        KeepAliveChannel::new(timer_client.clone(), KEEPALIVE_TICKS, spawner.clone());

    // Note that this transform does not contain the version prefix, as it is applied to a
    // connection between two nodes, relayed using a relay server.
    FuncFutTransform::new(move |(opt_public_key, conn_pair_vec)| {
        let mut c_encrypt_transform = encrypt_transform.clone();
        let mut c_keepalive_transform = keepalive_transform.clone();
        Box::pin(async move {
            let (public_key, conn_pair_vec) = c_encrypt_transform
                .transform((opt_public_key, conn_pair_vec))
                .await?;
            let conn_pair_vec = c_keepalive_transform.transform(conn_pair_vec).await;
            Some((public_key, conn_pair_vec))
        })
    })
}
