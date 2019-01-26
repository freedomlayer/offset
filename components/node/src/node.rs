use std::marker::Unpin;
use core::pin::Pin;

use futures::task::{Spawn, SpawnExt};
use futures::{Future, Stream};
use futures::channel::mpsc;

use common::conn::{ConnPair, ConnPairVec, FutTransform, BoxFuture};
use crypto::crypto_rand::CryptoRandom;
use crypto::identity::PublicKey;

use database::DatabaseClient;
use identity::IdentityClient;
use timer::TimerClient;

use app_server::IncomingAppConnection;
use channeler::{channeler_loop, PoolConnector, PoolListener};
use relay::client::{client_connector::ClientConnector, client_listener::ClientListener};
use keepalive::KeepAliveChannel;
use secure_channel::SecureChannel;

use proto::funder::messages::{RelayAddress, TcpAddress};
use proto::index_server::messages::{IndexServerAddress};

use crate::types::NodeMutation;

#[derive(Debug)]
pub enum NodeError {
    RequestPublicKeyError,
}

pub struct NodeConfig {
    channel_len: usize,
    backoff_ticks: usize,
    keepalive_ticks: usize,
    ticks_to_rekey: usize,
    max_concurrent_encrypt: usize,
    conn_timeout_ticks: usize,
}

#[derive(Clone)]
/// Open an encrypted connection to a relay endpoint
struct EncRelayConnector<ET,C> {
    encrypt_transform: ET,
    net_connector: C,
}

impl<ET,C> EncRelayConnector<ET,C> {
    pub fn new(encrypt_transform: ET,
               net_connector: C) -> Self {

        EncRelayConnector {
            encrypt_transform,
            net_connector,
        }
    }
}

impl<ET,C> FutTransform for EncRelayConnector<ET,C> 
where
    C: FutTransform<Input=TcpAddress,Output=Option<ConnPairVec>> + Clone + Send,
    ET: FutTransform<Input=(Option<PublicKey>, ConnPairVec),Output=Option<(PublicKey, ConnPairVec)>> + Send,
{
    type Input = RelayAddress;
    type Output = Option<ConnPairVec>;

    fn transform(&mut self, relay_address: Self::Input)
        -> BoxFuture<'_, Self::Output> {

        Box::pin(async move {
            let conn_pair = await!(self.net_connector.transform(relay_address.address))?;
            let (public_key, conn_pair) = await!(self.encrypt_transform.transform((Some(relay_address.public_key), conn_pair)))?;
            Some(conn_pair)
        })
    }
}

/// A connection style encrypt transform.
/// Does not return the public key of the remote side, because we already know it.
#[derive(Clone)]
struct ConnectEncryptTransform<ET> {
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
/// Returns the public key of the remote side
#[derive(Clone)]
struct ListenEncryptTransform<ET> {
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

pub async fn node_loop<C,IA,R,S>(
                node_config: NodeConfig,
                identity_client: IdentityClient,
                timer_client: TimerClient,
                database_client: DatabaseClient<NodeMutation<RelayAddress,IndexServerAddress>>,
                mut net_connector: C,
                incoming_apps: IA,
                rng: R,
                mut spawner: S) -> Result<(), NodeError> 
where
    R: CryptoRandom + Clone + 'static,
    S: Spawn + Clone + Send + Sync + 'static,
    C: FutTransform<Input=TcpAddress,Output=Option<ConnPairVec>> + Clone + Send + Sync + 'static,
    IA: Stream<Item=IncomingAppConnection<RelayAddress,IndexServerAddress>> + Unpin, 
{
    // Get local public key:
    let local_public_key = await!(identity_client.request_public_key())
        .map_err(|_| NodeError::RequestPublicKeyError)?;

    let encrypt_transform = SecureChannel::new(
        identity_client.clone(),
        rng.clone(),
        timer_client.clone(),
        node_config.ticks_to_rekey,
        spawner.clone());

    let keepalive_transform = KeepAliveChannel::new(
        timer_client.clone(),
        node_config.keepalive_ticks,
        spawner.clone());


    let enc_relay_connector = EncRelayConnector::new(encrypt_transform.clone(), net_connector);

    let client_connector = ClientConnector::new(enc_relay_connector.clone(), 
                                                keepalive_transform.clone());

    let connect_encrypt_transform = ConnectEncryptTransform::new(
        encrypt_transform.clone());

    let pool_connector = PoolConnector::new(
           timer_client.clone(),
           client_connector.clone(),
           connect_encrypt_transform,
           node_config.backoff_ticks,
           spawner.clone());



    let client_listener = ClientListener::new(
           enc_relay_connector,
           keepalive_transform.clone(),
           node_config.conn_timeout_ticks,
           timer_client.clone(),
           spawner.clone());

    let listen_encrypt_transform = ListenEncryptTransform::new(
        encrypt_transform.clone());

    let pool_listener = PoolListener::<RelayAddress,_,_,_>::new(client_listener,
           listen_encrypt_transform,
           node_config.max_concurrent_encrypt,
           node_config.backoff_ticks,
           timer_client.clone(),
           spawner.clone());

    let (channeler_to_funder_sender, channeler_to_funder_receiver) = mpsc::channel(node_config.channel_len);
    let (funder_to_channeler_sender, funder_to_channeler_receiver) = mpsc::channel(node_config.channel_len);

    let channeler_fut = channeler_loop(
                            local_public_key.clone(),
                            funder_to_channeler_receiver,
                            channeler_to_funder_sender,
                            pool_connector,
                            pool_listener,
                            spawner.clone());

    spawner.spawn_with_handle(channeler_fut);

    // TODO:
    // - Spawn Channeler
    // - Spawn Funder
    // - Spawn IndexClient
    // - Spawn AppServer

    unimplemented!();
}
