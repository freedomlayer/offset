use futures::task::Spawn;

use common::conn::{ConnPairVec, FuncFutTransform, FutTransform};

use proto::consts::{KEEPALIVE_TICKS, PROTOCOL_VERSION, TICKS_TO_REKEY};
use proto::crypto::PublicKey;
use proto::net::messages::NetAddress;

use crypto::rand::CryptoRandom;

use identity::IdentityClient;
use timer::TimerClient;

use keepalive::KeepAliveChannel;
use secure_channel::SecureChannel;
use version::VersionPrefix;

/// Create an encrypt-keepalive transformation:
/// Composes: Encryption * Keepalive
pub fn create_encrypt_keepalive<R, S>(
    timer_client: TimerClient,
    identity_client: IdentityClient,
    rng: R,
    spawner: S,
) -> impl FutTransform<
    Input = (Option<PublicKey>, ConnPairVec),
    Output = Option<(PublicKey, ConnPairVec)>,
> + Clone
       + Send
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
    let keepalive_transform = KeepAliveChannel::new(timer_client, KEEPALIVE_TICKS, spawner);

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

/// Turn a regular connector into a secure connector.
/// Composes: Version * Encryption * Keepalive
pub fn create_version_encrypt_keepalive<R, S>(
    timer_client: TimerClient,
    identity_client: IdentityClient,
    rng: R,
    spawner: S,
) -> impl FutTransform<
    Input = (Option<PublicKey>, ConnPairVec),
    Output = Option<(PublicKey, ConnPairVec)>,
> + Clone
       + Send
where
    S: Spawn + Clone + Send + 'static,
    R: CryptoRandom + Clone + 'static,
{
    // Wrap the connection (Version * Encrypt * Keepalive):
    let version_transform = VersionPrefix::new(PROTOCOL_VERSION, spawner.clone());
    let encrypt_transform = SecureChannel::new(
        identity_client,
        rng,
        timer_client.clone(),
        TICKS_TO_REKEY,
        spawner.clone(),
    );
    let keepalive_transform = KeepAliveChannel::new(timer_client, KEEPALIVE_TICKS, spawner);

    FuncFutTransform::new(move |(opt_public_key, conn_pair)| {
        let mut c_version_transform = version_transform.clone();
        let mut c_encrypt_transform = encrypt_transform.clone();
        let mut c_keepalive_transform = keepalive_transform.clone();
        Box::pin(async move {
            let conn_pair = c_version_transform.transform(conn_pair).await;
            let (public_key, conn_pair) = c_encrypt_transform
                .transform((opt_public_key, conn_pair))
                .await?;
            let conn_pair = c_keepalive_transform.transform(conn_pair).await;
            Some((public_key, conn_pair))
        })
    })
}

// TODO: Possibly remove in favour of create_version_encrypt_keepalive
/// Turn a regular connector into a secure connector.
/// Composes: Version * Encryption * Keepalive
pub fn create_secure_connector<C, R, S>(
    connector: C,
    timer_client: TimerClient,
    identity_client: IdentityClient,
    rng: R,
    spawner: S,
) -> impl FutTransform<Input = (PublicKey, NetAddress), Output = Option<ConnPairVec>> + Clone
where
    S: Spawn + Clone + Send + 'static,
    R: CryptoRandom + Clone + 'static,
    C: FutTransform<Input = NetAddress, Output = Option<ConnPairVec>> + Clone + Send + 'static,
{
    let conn_transform =
        create_version_encrypt_keepalive(timer_client, identity_client, rng, spawner);

    FuncFutTransform::new(move |(public_key, net_address)| {
        let mut c_connector = connector.clone();
        let mut c_conn_transform = conn_transform.clone();
        Box::pin(async move {
            let conn_pair = c_connector.transform(net_address).await?;
            let (_public_key, conn_pair) = c_conn_transform
                .transform((Some(public_key), conn_pair))
                .await?;
            Some(conn_pair)
        })
    })
}
