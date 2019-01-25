use std::marker::Unpin;

use futures::task::Spawn;
use futures::Stream;
use futures::channel::mpsc;

use common::conn::{ConnPair, ConnPairVec, FutTransform};
use crypto::crypto_rand::CryptoRandom;

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
}

pub async fn node_loop<C,IA,R,S>(
                node_config: NodeConfig,
                identity_client: IdentityClient,
                timer_client: TimerClient,
                database_client: DatabaseClient<NodeMutation<RelayAddress,IndexServerAddress>>,
                net_connector: C,
                incoming_apps: IA,
                rng: R,
                spawner: S) -> Result<(), NodeError> 
where
    R: CryptoRandom + Clone,
    S: Spawn + Clone + Send,
    C: FutTransform<Input=TcpAddress,Output=Option<ConnPairVec>>,
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

    // TODO: compose encryption and keepalive over net_connector.
    let relay_connector = ClientConnector::new(
        net_connector,
        keepalive_transform.clone());

    // C: FutTransform<Input=(RelayAddress, PublicKey), Output=Option<RawConn>> + Clone + Send + 'static,

    /*
    let pool_connector = PoolConnector::new(
           timer_client.clone(),
           client_connector: C,
           encrypt_transform: ET,
           node_config.backoff_ticks,
           spawner.clone());
           */

    /*
    let (channeler_to_funder_sender, channeler_to_funder_receiver) = mpsc::channel(node_config.channel_len);
    let (funder_to_channeler_sender, funder_to_channeler_receiver) = mpsc::channel(node_config.channel_len);

    // C: FutTransform<Input=PublicKey, Output=ConnectPoolControl<B>> + Clone + Send + Sync + 'static,
    // L: Listener<Connection=(PublicKey, RawConn), Config=LpConfig<B>, Arg=()> + Clone + Send,

    let channeler_fut = channeler_loop(
                            local_public_key.clone(),
                            funder_to_channeler_receiver,
                            channeler_to_funder_sender,
                            connector: C,
                            listener: L,
                            spawner.clone());

    spawner.spawn_with_handle(channeler_fut);
    */

    // TODO:
    // - Spawn Channeler
    // - Spawn Funder
    // - Spawn IndexClient
    // - Spawn AppServer

    unimplemented!();
}
