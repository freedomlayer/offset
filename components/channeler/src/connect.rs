use futures::task::Spawn;

use crypto::identity::PublicKey;

use timer::TimerClient;
use timer::utils::sleep_ticks;

use common::conn::{Connector, ConnPair, BoxFuture, ConnTransform};
use relay::client::client_connector::ClientConnector;


async fn secure_connect<C,T,A,S>(mut client_connector: C,
                            mut encrypt_transform: T,
                            address: A,
                            public_key: PublicKey,
                            spawner: S) -> Option<ConnPair<Vec<u8>, Vec<u8>>>
where
    A: Clone,
    C: Connector<Address=(A, PublicKey), SendItem=Vec<u8>, RecvItem=Vec<u8>>,
    T: ConnTransform<OldSendItem=Vec<u8>,OldRecvItem=Vec<u8>,
                     NewSendItem=Vec<u8>,NewRecvItem=Vec<u8>, 
                     Arg=Option<PublicKey>>,
    S: Spawn + Clone + Sync + Send,
{
    let (sender, receiver) = await!(client_connector.connect((address, public_key.clone())))?;
    await!(encrypt_transform.transform(Some(public_key), (sender, receiver)))
}

#[derive(Clone)]
pub struct ChannelerConnector<C,T,S> {
    client_connector: C,
    encrypt_transform: T,
    backoff_ticks: usize,
    timer_client: TimerClient,
    spawner: S,
}

impl<C,T,S> ChannelerConnector<C,T,S> {
    pub fn new(client_connector: C,
               encrypt_transform: T,
               backoff_ticks: usize,
               timer_client: TimerClient,
               spawner: S) -> ChannelerConnector<C,T,S> {

        ChannelerConnector {
            client_connector,
            encrypt_transform,
            backoff_ticks,
            timer_client,
            spawner,
        }
    }
}

impl<A,C,T,S> Connector for ChannelerConnector<C,T,S> 
where
    A: Sync + Send + Clone + 'static,
    C: Connector<Address=(A, PublicKey), SendItem=Vec<u8>, RecvItem=Vec<u8>> + Clone + Send + Sync + 'static,
    T: ConnTransform<OldSendItem=Vec<u8>,OldRecvItem=Vec<u8>,
                     NewSendItem=Vec<u8>,NewRecvItem=Vec<u8>, 
                     Arg=Option<PublicKey>> + Clone + Send,
    S: Spawn + Clone + Sync + Send,
{
    type Address = (A, PublicKey);
    type SendItem = Vec<u8>;
    type RecvItem = Vec<u8>;

    fn connect(&mut self, address: (A, PublicKey)) 
        -> BoxFuture<'_, Option<ConnPair<Vec<u8>, Vec<u8>>>> {

        let (relay_address, public_key) = address;

        Box::pinned(async move {
            loop {
                match await!(secure_connect(self.client_connector.clone(), self.encrypt_transform.clone(), 
                                            relay_address.clone(), public_key.clone(), 
                                            self.spawner.clone())) {
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

    use timer::create_timer_incoming;
    use crypto::crypto_rand::RngContainer;

    use common::dummy_connector::{DummyConnector, ConnRequest};

    async fn task_channeler_connector_basic(mut spawner: impl Spawn + Clone) {

        // Create a mock time service:
        let (tick_sender, tick_receiver) = mpsc::channel::<()>(0);
        let timer_client = create_timer_incoming(tick_receiver, spawner.clone()).unwrap();

        let backoff_ticks = 2;

        let (conn_request_sender, conn_request_receiver) = mpsc::channel::<ConnRequest<Vec<u8>,Vec<u8>,u32>>(0);
        let connector = DummyConnector::new(conn_request_sender);

        /*
        let channeler_connector = ChannelerConnector::new(
            client_connector,
            encrypt_transform,
            backoff_ticks,
            timer_client,
            spawner);
            */

        // channeler_connector.connect((0x1337, public_key));
        // TODO: Finish here

    }

    #[test]
    fn test_channeler_connector_basic() {
        let mut thread_pool = ThreadPool::new().unwrap();
        thread_pool.run(task_channeler_connector_basic(thread_pool.clone()));
    }
}
