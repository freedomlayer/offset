use futures::channel::oneshot;
use futures::task::Spawn;
use futures::{FutureExt, select};

use proto::consts::TICKS_TO_REKEY;

use crypto::identity::PublicKey;
use crypto::crypto_rand::CryptoRandom;

use identity::IdentityClient;

use timer::TimerClient;
use timer::utils::sleep_ticks;

use relay::client::connector::{Connector, ConnPair, BoxFuture};
use relay::client::client_connector::ClientConnector;

use secure_channel::create_secure_channel;


#[derive(Debug)]
pub enum ConnectError {
    Canceled,
}

enum ConnectSelect {
    Canceled,
    ConnectFailed,
    Connected(ConnPair<Vec<u8>,Vec<u8>>),
}

async fn secure_connect<C,A,R,S>(mut client_connector: C,
                            timer_client: TimerClient,
                            address: A,
                            public_key: PublicKey,
                            identity_client: IdentityClient,
                            rng: R,
                            spawner: S) -> Option<ConnPair<Vec<u8>, Vec<u8>>>
where
    A: Clone,
    C: Connector<Address=(A, PublicKey), SendItem=Vec<u8>, RecvItem=Vec<u8>>,
    R: CryptoRandom + 'static,
    S: Spawn + Clone + Sync + Send,
{
    let conn_pair = await!(client_connector.connect((address, public_key.clone())))?;
    match await!(create_secure_channel(conn_pair.sender, conn_pair.receiver,
                          identity_client,
                          Some(public_key.clone()),
                          rng,
                          timer_client,
                          TICKS_TO_REKEY,
                          spawner)) {
        Ok((sender, receiver)) => Some(ConnPair {sender, receiver}),
        Err(e) => {
            error!("Error in create_secure_channel: {:?}", e);
            None
        },
    }
}

pub struct ChannelerConnector<C,R,S> {
    connector: C,
    keepalive_ticks: usize,
    backoff_ticks: usize,
    timer_client: TimerClient,
    identity_client: IdentityClient,
    rng: R,
    spawner: S,
}

impl<C,R,S> ChannelerConnector<C,R,S> {
    pub fn new(connector: C,
               keepalive_ticks: usize,
               backoff_ticks: usize,
               timer_client: TimerClient,
               identity_client: IdentityClient,
               rng: R,
               spawner: S) -> ChannelerConnector<C,R,S> {

        ChannelerConnector {
            connector,
            keepalive_ticks,
            backoff_ticks,
            timer_client,
            identity_client,
            rng,
            spawner,
        }
    }
}

impl<A,C,R,S> Connector for ChannelerConnector<C,R,S> 
where
    A: Sync + Send + Clone + 'static,
    C: Connector<Address=A, SendItem=Vec<u8>, RecvItem=Vec<u8>> + Clone + Send + Sync + 'static,
    R: CryptoRandom + 'static,
    S: Spawn + Clone + Sync + Send,
{
    type Address = (A, PublicKey);
    type SendItem = Vec<u8>;
    type RecvItem = Vec<u8>;

    fn connect(&mut self, address: (A, PublicKey)) 
        -> BoxFuture<'_, Option<ConnPair<Vec<u8>, Vec<u8>>>> {

        let (relay_address, public_key) = address;

        let client_connector = ClientConnector::new(
            self.connector.clone(), self.spawner.clone(), self.timer_client.clone(), self.keepalive_ticks);

        Box::pinned(async move {
            loop {
                match await!(secure_connect(client_connector.clone(), self.timer_client.clone(), relay_address.clone(), 
                                               public_key.clone(), self.identity_client.clone(), self.rng.clone(), self.spawner.clone())) {
                    Some(conn_pair) => return Some(conn_pair),
                    None => await!(sleep_ticks(self.backoff_ticks, self.timer_client.clone())).unwrap(),
                }
            }
        })
    }
}



/*
pub async fn connect<A,C,S,R>(connector: C,
                address: A, 
                public_key: PublicKey, 
                keepalive_ticks: usize,
                backoff_ticks: usize,
                timer_client: TimerClient,
                close_receiver: oneshot::Receiver<()>,
                identity_client: IdentityClient,
                rng: R,
                spawner: S) -> Result<ConnPair<Vec<u8>, Vec<u8>>, ConnectError>
where
    A: Sync + Send + Clone + 'static,
    C: Connector<Address=A, SendItem=Vec<u8>, RecvItem=Vec<u8>> + Clone + Send + Sync + 'static,
    R: CryptoRandom + 'static,
    S: Spawn + Clone + Sync + Send,
{
    let mut close_receiver = close_receiver.map(|_| ConnectSelect::Canceled).fuse();

    let client_connector = ClientConnector::new(
        connector.clone(), spawner.clone(), timer_client.clone(), keepalive_ticks);

    let mut connect_fut = Box::pinned(async move { 
        loop {
            match await!(secure_connect(client_connector.clone(), timer_client.clone(), address.clone(), 
                                           public_key.clone(), identity_client.clone(), rng.clone(), spawner.clone())) {
                Some(conn_pair) => return conn_pair,
                None => await!(sleep_ticks(backoff_ticks, timer_client.clone())).unwrap(),
            }
        }
    }).fuse();

    select! {
        connect_fut = connect_fut => Ok(connect_fut),
        _close_receiver = close_receiver => Err(ConnectError::Canceled),
    }
}
*/

