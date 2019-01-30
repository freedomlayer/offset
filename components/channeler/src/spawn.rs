use std::fmt::Debug;
use std::hash::Hash;

use futures::channel::mpsc;
use futures::task::{Spawn, SpawnExt};
use futures::{Future};

use common::conn::{FutTransform, ConnPairVec, BoxFuture};
use timer::TimerClient;

use crypto::identity::PublicKey;

use relay::client::client_connector::ClientConnector;
use relay::client::client_listener::ClientListener;

use proto::funder::messages::{FunderToChanneler, ChannelerToFunder};
use crate::channeler::{channeler_loop, ChannelerError};
use crate::connect_pool::PoolConnector;
use crate::listen_pool::PoolListener;


/// A connection style encrypt transform.
/// Does not return the public key of the remote side, because we already know it.
#[derive(Clone)]
pub struct ConnectEncryptTransform<ET> {
    encrypt_transform: ET,
}

impl<ET> ConnectEncryptTransform<ET> {
    pub fn new(encrypt_transform: ET) -> Self {

        ConnectEncryptTransform {
            encrypt_transform,
        }
    }
}

impl<ET> FutTransform for ConnectEncryptTransform<ET> 
where
    ET: FutTransform<Input=(Option<PublicKey>, ConnPairVec),Output=Option<(PublicKey, ConnPairVec)>> + Send,
{
    type Input = (PublicKey, ConnPairVec);
    type Output = Option<ConnPairVec>;

    fn transform(&mut self, input: Self::Input)
        -> BoxFuture<'_, Self::Output> {

        let (public_key, conn_pair) = input;

        Box::pin(async move {
            let (_public_key, conn_pair) = await!(
                self.encrypt_transform.transform((Some(public_key), conn_pair)))?;
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

        ListenEncryptTransform {
            encrypt_transform,
        }
    }
}

impl<ET> FutTransform for ListenEncryptTransform<ET> 
where
    ET: FutTransform<Input=(Option<PublicKey>, ConnPairVec),Output=Option<(PublicKey, ConnPairVec)>> + Send,
{
    type Input = (PublicKey, ConnPairVec);
    type Output = Option<(PublicKey, ConnPairVec)>;

    fn transform(&mut self, input: Self::Input)
        -> BoxFuture<'_, Self::Output> {

        let (public_key, conn_pair) = input;

        Box::pin(async move {
            await!(self.encrypt_transform.transform((Some(public_key), conn_pair)))
        })
    }
}


#[derive(Debug)]
pub enum SpawnChannelerError {
    SpawnError,
}

pub fn spawn_channeler<B,C,ET,KT,S>(local_public_key: PublicKey,
                          timer_client: TimerClient,
                          backoff_ticks: usize,
                          conn_timeout_ticks: usize,
                          max_concurrent_encrypt: usize,
                          enc_relay_connector: C,
                          encrypt_transform: ET,
                          keepalive_transform: KT,
                          from_funder: mpsc::Receiver<FunderToChanneler<Vec<B>>>,
                          to_funder: mpsc::Sender<ChannelerToFunder>,
                          mut spawner: S) 
    -> Result<impl Future<Output=Result<(), ChannelerError>>, SpawnChannelerError>

where
    B: Eq + Hash + Clone + Send + Sync + Debug + 'static,
    C: FutTransform<Input=B,Output=Option<ConnPairVec>> + Clone + Send + Sync + 'static,
    ET: FutTransform<Input=(Option<PublicKey>, ConnPairVec),Output=Option<(PublicKey, ConnPairVec)>> + Clone + Send + Sync + 'static,
    KT: FutTransform<Input=ConnPairVec,Output=ConnPairVec> + Clone + Send + Sync + 'static,
    S: Spawn + Clone + Send + Sync + 'static,
{

    let client_connector = ClientConnector::new(enc_relay_connector.clone(), 
                                                keepalive_transform.clone());

    let connect_encrypt_transform = ConnectEncryptTransform::new(
        encrypt_transform.clone());

    let pool_connector = PoolConnector::new(
           timer_client.clone(),
           client_connector.clone(),
           connect_encrypt_transform,
           backoff_ticks,
           spawner.clone());

    let client_listener = ClientListener::new(
           enc_relay_connector,
           keepalive_transform.clone(),
           conn_timeout_ticks,
           timer_client.clone(),
           spawner.clone());

    let listen_encrypt_transform = ListenEncryptTransform::new(
        encrypt_transform.clone());

    let pool_listener = PoolListener::<B,_,_,_>::new(client_listener,
           listen_encrypt_transform,
           max_concurrent_encrypt,
           backoff_ticks,
           timer_client.clone(),
           spawner.clone());

    let channeler_fut = channeler_loop(
                            local_public_key,
                            from_funder,
                            to_funder,
                            pool_connector,
                            pool_listener,
                            spawner.clone());

    spawner.spawn_with_handle(channeler_fut)
        .map_err(|_| SpawnChannelerError::SpawnError)
}
