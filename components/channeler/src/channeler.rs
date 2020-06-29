use std::fmt::Debug;
use std::hash::Hash;

use futures::channel::mpsc;
use futures::task::Spawn;

use common::conn::{BoxFuture, ConnPairVec, FutTransform};
use timer::TimerClient;

use proto::crypto::PublicKey;
use proto::funder::messages::{ChannelerToFunder, FunderToChanneler};

use relay::{ClientConnector, ClientListener};

use crate::connect_pool::PoolConnector;
use crate::inner_loop::{channeler_loop_inner, ChannelerError};
use crate::listen_pool::PoolListener;

/// A connection style encrypt transform.
/// Does not return the public key of the remote side, because we already know it.
#[derive(Clone)]
pub struct ConnectEncryptTransform<ET> {
    encrypt_transform: ET,
}

impl<ET> ConnectEncryptTransform<ET> {
    pub fn new(encrypt_transform: ET) -> Self {
        ConnectEncryptTransform { encrypt_transform }
    }
}

impl<ET> FutTransform for ConnectEncryptTransform<ET>
where
    ET: FutTransform<
            Input = (Option<PublicKey>, ConnPairVec),
            Output = Option<(PublicKey, ConnPairVec)>,
        > + Send,
{
    type Input = (PublicKey, ConnPairVec);
    type Output = Option<ConnPairVec>;

    fn transform(&mut self, input: Self::Input) -> BoxFuture<'_, Self::Output> {
        let (public_key, conn_pair) = input;

        Box::pin(async move {
            let (_public_key, conn_pair) = self
                .encrypt_transform
                .transform((Some(public_key), conn_pair))
                .await?;
            Some(conn_pair)
        })
    }
}

/// A Listen style encrypt transform.
/// Returns the public key of the remote side, because we can not predict it.
#[derive(Clone)]
pub struct ListenEncryptTransform<ET> {
    encrypt_transform: ET,
}

impl<ET> ListenEncryptTransform<ET> {
    pub fn new(encrypt_transform: ET) -> Self {
        ListenEncryptTransform { encrypt_transform }
    }
}

impl<ET> FutTransform for ListenEncryptTransform<ET>
where
    ET: FutTransform<
            Input = (Option<PublicKey>, ConnPairVec),
            Output = Option<(PublicKey, ConnPairVec)>,
        > + Send,
{
    type Input = (PublicKey, ConnPairVec);
    type Output = Option<(PublicKey, ConnPairVec)>;

    fn transform(&mut self, input: Self::Input) -> BoxFuture<'_, Self::Output> {
        let (public_key, conn_pair) = input;

        Box::pin(async move {
            self.encrypt_transform
                .transform((Some(public_key), conn_pair))
                .await
        })
    }
}

#[derive(Debug)]
pub enum SpawnChannelerError {
    SpawnError,
}

pub async fn channeler_loop<RA, C, EKT, S>(
    local_public_key: PublicKey,
    timer_client: TimerClient,
    backoff_ticks: usize,
    conn_timeout_ticks: usize,
    max_concurrent_encrypt: usize,
    connector: C,
    encrypt_keepalive: EKT,
    from_funder: mpsc::Receiver<FunderToChanneler<RA>>,
    to_funder: mpsc::Sender<ChannelerToFunder>,
    spawner: S,
) -> Result<(), ChannelerError>
where
    RA: Eq + Hash + Clone + Send + Sync + Debug + 'static,
    C: FutTransform<Input = RA, Output = Option<ConnPairVec>> + Clone + Send + 'static,
    EKT: FutTransform<
            Input = (Option<PublicKey>, ConnPairVec),
            Output = Option<(PublicKey, ConnPairVec)>,
        > + Clone
        + Send
        + 'static,
    S: Spawn + Clone + Send + 'static,
{
    let client_connector = ClientConnector::new(connector.clone());

    let connect_encrypt_transform = ConnectEncryptTransform::new(encrypt_keepalive.clone());

    let pool_connector = PoolConnector::new(
        timer_client.clone(),
        client_connector.clone(),
        connect_encrypt_transform,
        backoff_ticks,
        spawner.clone(),
    );

    let client_listener = ClientListener::new(
        connector,
        conn_timeout_ticks,
        timer_client.clone(),
        spawner.clone(),
    );

    let listen_encrypt_transform = ListenEncryptTransform::new(encrypt_keepalive.clone());

    let pool_listener = PoolListener::<RA, _, _, _>::new(
        client_listener,
        listen_encrypt_transform,
        max_concurrent_encrypt,
        backoff_ticks,
        timer_client.clone(),
        spawner.clone(),
    );

    // A hack to explain to the compiler that spawner (S) doesn't need to be Sync.
    let c_spawner = spawner.clone();
    channeler_loop_inner(
        local_public_key,
        from_funder,
        to_funder,
        pool_connector,
        pool_listener,
        c_spawner,
    )
    .await
}
