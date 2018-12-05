use futures::task::Spawn;

use proto::consts::TICKS_TO_REKEY;

use crypto::identity::PublicKey;
use crypto::crypto_rand::CryptoRandom;

use identity::IdentityClient;

use timer::TimerClient;
use timer::utils::sleep_ticks;

use common::conn::{Connector, ConnPair, BoxFuture, ConnTransform};
use relay::client::client_connector::ClientConnector;


async fn secure_connect<C,T,A,R,S>(mut client_connector: C,
                            encrypt_transform: T,
                            timer_client: TimerClient,
                            address: A,
                            public_key: PublicKey,
                            identity_client: IdentityClient,
                            rng: R,
                            spawner: S) -> Option<ConnPair<Vec<u8>, Vec<u8>>>
where
    A: Clone,
    C: Connector<Address=(A, PublicKey), SendItem=Vec<u8>, RecvItem=Vec<u8>>,
    T: ConnTransform<OldSendItem=Vec<u8>,OldRecvItem=Vec<u8>,
                     NewSendItem=Vec<u8>,NewRecvItem=Vec<u8>, 
                     Arg=Option<PublicKey>>,
    R: CryptoRandom + 'static,
    S: Spawn + Clone + Sync + Send,
{
    let (sender, receiver) = await!(client_connector.connect((address, public_key.clone())))?;
    await!(encrypt_transform.transform(Some(public_key), (sender, receiver)))
}

#[derive(Clone)]
pub struct ChannelerConnector<C,T,R,S> {
    connector: C,
    encrypt_transform: T,
    keepalive_ticks: usize,
    backoff_ticks: usize,
    timer_client: TimerClient,
    identity_client: IdentityClient,
    rng: R,
    spawner: S,
}

impl<C,T,R,S> ChannelerConnector<C,T,R,S> {
    pub fn new(connector: C,
               encrypt_transform: T,
               keepalive_ticks: usize,
               backoff_ticks: usize,
               timer_client: TimerClient,
               identity_client: IdentityClient,
               rng: R,
               spawner: S) -> ChannelerConnector<C,T,R,S> {

        ChannelerConnector {
            connector,
            encrypt_transform,
            keepalive_ticks,
            backoff_ticks,
            timer_client,
            identity_client,
            rng,
            spawner,
        }
    }
}

impl<A,C,T,R,S> Connector for ChannelerConnector<C,T,R,S> 
where
    A: Sync + Send + Clone + 'static,
    C: Connector<Address=A, SendItem=Vec<u8>, RecvItem=Vec<u8>> + Clone + Send + Sync + 'static,
    T: ConnTransform<OldSendItem=Vec<u8>,OldRecvItem=Vec<u8>,
                     NewSendItem=Vec<u8>,NewRecvItem=Vec<u8>, 
                     Arg=Option<PublicKey>> + Clone + Send,
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
                match await!(secure_connect(client_connector.clone(), self.encrypt_transform.clone(), 
                                            self.timer_client.clone(), relay_address.clone(), 
                                            public_key.clone(), self.identity_client.clone(), self.rng.clone(), self.spawner.clone())) {
                    Some(conn_pair) => return Some(conn_pair),
                    None => await!(sleep_ticks(self.backoff_ticks, self.timer_client.clone())).unwrap(),
                }
            }
        })
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use futures::executor::ThreadPool;
    use futures::channel::mpsc;
    use futures::FutureExt;
    use futures::task::SpawnExt;

    use crypto::test_utils::DummyRandom;
    use crypto::identity::{SoftwareEd25519Identity,
                            generate_pkcs8_key_pair, PUBLIC_KEY_LEN,
                            PublicKey};
    use identity::create_identity;
    use timer::create_timer_incoming;
    use crypto::crypto_rand::RngContainer;

    use common::dummy_connector::{DummyConnector, ConnRequest};

    async fn task_channeler_connector_basic(mut spawner: impl Spawn + Clone) {
        /*

        // Create a mock time service:
        let (tick_sender, tick_receiver) = mpsc::channel::<()>(0);
        let timer_client = create_timer_incoming(tick_receiver, spawner.clone()).unwrap();

        let rng = RngContainer::new(DummyRandom::new(&[1u8]));
        let pkcs8 = generate_pkcs8_key_pair(&rng);
        let identity = SoftwareEd25519Identity::from_pkcs8(&pkcs8).unwrap();
        let (requests_sender, identity_server) = create_identity(identity);
        let identity_client = IdentityClient::new(requests_sender);
        spawner.spawn(identity_server.map(|_| ())).unwrap();

        let public_key = await!(identity_client.request_public_key()).unwrap();

        let backoff_ticks = 2;
        let keepalive_ticks = 8;

        let (conn_request_sender, conn_request_receiver) = mpsc::channel::<ConnRequest<Vec<u8>,Vec<u8>,u32>>(0);
        let connector = DummyConnector::new(conn_request_sender);

        let channeler_connector = ChannelerConnector::new(
            connector,
            encrypt_transform,
            keepalive_ticks,
            backoff_ticks,
            timer_client,
            identity_client,
            rng,
            spawner);

        // channeler_connector.connect((0x1337, public_key));
        // TODO: Finish here
        */

    }

    #[test]
    fn test_channeler_connector_basic() {
        let mut thread_pool = ThreadPool::new().unwrap();
        thread_pool.run(task_channeler_connector_basic(thread_pool.clone()));
    }
}
