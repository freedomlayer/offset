use futures::task::Spawn;

use crypto::identity::PublicKey;

use timer::TimerClient;
use timer::utils::sleep_ticks;

use common::conn::{Connector, ConnPair, BoxFuture, ConnTransform};
use relay::client::client_connector::ClientConnector;


async fn secure_connect<C,T,A>(mut client_connector: C,
                            mut encrypt_transform: T,
                            address: A,
                            public_key: PublicKey) -> Option<ConnPair<Vec<u8>, Vec<u8>>>
where
    A: Clone,
    C: Connector<Address=(A, PublicKey), SendItem=Vec<u8>, RecvItem=Vec<u8>>,
    T: ConnTransform<OldSendItem=Vec<u8>,OldRecvItem=Vec<u8>,
                     NewSendItem=Vec<u8>,NewRecvItem=Vec<u8>, 
                     Arg=Option<PublicKey>>,
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
                                            relay_address.clone(), public_key.clone())) {
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
    use futures::{FutureExt, StreamExt, SinkExt};
    use futures::task::SpawnExt;

    use timer::{create_timer_incoming, dummy_timer_multi_sender, TimerTick};
    use crypto::crypto_rand::RngContainer;
    use crypto::identity::PUBLIC_KEY_LEN;

    use common::dummy_connector::{DummyConnector, ConnRequest};
    use common::conn::IdentityConnTransform;


    /// Check basic connection using the ChannelerConnector.
    async fn task_channeler_connector_basic<S>(mut spawner: S) 
    where
        S: Spawn + Clone + Sync + Send,
    {

        // Create a mock time service:
        let (tick_sender, tick_receiver) = mpsc::channel::<()>(0);
        let timer_client = create_timer_incoming(tick_receiver, spawner.clone()).unwrap();

        let backoff_ticks = 2;

        let (conn_request_sender, mut conn_request_receiver) = mpsc::channel::<ConnRequest<Vec<u8>,Vec<u8>,(u32, PublicKey)>>(0);
        let client_connector = DummyConnector::new(conn_request_sender);

        // We don't need encryption for this test:
        let encrypt_transform = IdentityConnTransform::<Vec<u8>,Vec<u8>,Option<PublicKey>>::new();

        let mut channeler_connector = ChannelerConnector::new(
            client_connector,
            encrypt_transform,
            backoff_ticks,
            timer_client,
            spawner);

        let public_key_b = PublicKey::from(&[0xaa; PUBLIC_KEY_LEN]);
        let connect_fut = async {
            let (mut sender, mut receiver) = await!(channeler_connector.connect((0x1337, public_key_b))).unwrap();
            await!(sender.send(vec![1,2,3])).unwrap();
            assert_eq!(await!(receiver.next()).unwrap(), vec![3,2,1]);
        };

        let server_fut = async {
            let request = await!(conn_request_receiver.next()).unwrap();
            let (mut local_sender, remote_receiver) = mpsc::channel(0);
            let (remote_sender, mut local_receiver) = mpsc::channel(0);
            request.reply(Some((remote_sender, remote_receiver)));
            assert_eq!(await!(local_receiver.next()).unwrap(), vec![1,2,3]);
            await!(local_sender.send(vec![3,2,1])).unwrap();
        };

        let _ = await!(connect_fut.join(server_fut));
    }

    #[test]
    fn test_channeler_connector_basic() {
        let mut thread_pool = ThreadPool::new().unwrap();
        thread_pool.run(task_channeler_connector_basic(thread_pool.clone()));
    }


    /// Check connection retry in case of connection failure
    async fn task_channeler_connector_retry<S>(mut spawner: S) 
    where
        S: Spawn + Clone + Sync + Send,
    {

        // Create a mock time service:
        let (mut tick_sender_receiver, timer_client) = dummy_timer_multi_sender(spawner.clone());

        let backoff_ticks = 2;

        let (conn_request_sender, mut conn_request_receiver) = mpsc::channel::<ConnRequest<Vec<u8>,Vec<u8>,(u32, PublicKey)>>(0);
        let client_connector = DummyConnector::new(conn_request_sender);

        // We don't need encryption for this test:
        let encrypt_transform = IdentityConnTransform::<Vec<u8>,Vec<u8>,Option<PublicKey>>::new();

        let mut channeler_connector = ChannelerConnector::new(
            client_connector,
            encrypt_transform,
            backoff_ticks,
            timer_client,
            spawner);

        let public_key_b = PublicKey::from(&[0xaa; PUBLIC_KEY_LEN]);
        let connect_fut = async {
            let (mut sender, mut receiver) = await!(channeler_connector.connect((0x1337, public_key_b))).unwrap();
            await!(sender.send(vec![1,2,3])).unwrap();
            assert_eq!(await!(receiver.next()).unwrap(), vec![3,2,1]);
        };

        let server_fut = async {
            // Fail in the first time:
            let request = await!(conn_request_receiver.next()).unwrap();
            request.reply(None);
            // Wait for a request for a timer stream:
            let mut tick_sender = await!(tick_sender_receiver.next()).unwrap();
            // Wait enough time to let the client try again:
            for _ in 0 .. backoff_ticks { 
                await!(tick_sender.send(TimerTick)).unwrap();
            }

            // This time return a connection:
            let request = await!(conn_request_receiver.next()).unwrap();
            let (mut local_sender, remote_receiver) = mpsc::channel(0);
            let (remote_sender, mut local_receiver) = mpsc::channel(0);
            request.reply(Some((remote_sender, remote_receiver)));
            assert_eq!(await!(local_receiver.next()).unwrap(), vec![1,2,3]);
            await!(local_sender.send(vec![3,2,1])).unwrap();
        };

        let _ = await!(connect_fut.join(server_fut));
    }

    #[test]
    fn test_channeler_connector_retry() {
        let mut thread_pool = ThreadPool::new().unwrap();
        thread_pool.run(task_channeler_connector_retry(thread_pool.clone()));
    }
}
