use crypto::identity::PublicKey;
use futures::{future, FutureExt, TryFutureExt, StreamExt, SinkExt};
use futures::future::FutureObj;
use futures::task::{Spawn, SpawnExt};
use futures::channel::mpsc;

use proto::relay::messages::{InitConnection};
use proto::relay::serialize::{serialize_init_connection, serialize_tunnel_message,
    deserialize_tunnel_message};

use timer::TimerClient;

use super::client_tunnel::client_tunnel;
use super::connector::{Connector, ConnPair};

#[derive(Debug)]
pub enum ClientConnectorError {
    InnerConnectorError,
    SendInitConnectionError,
    RequestTimerStreamError,
    SpawnClientTunnelError,
}

pub struct ClientConnector<C,S> {
    connector: C,
    spawner: S,
    timer_client: TimerClient,
    keepalive_ticks: usize,
}

impl<A: 'static,C,S> ClientConnector<C,S> 
where
    C: Connector<Address=A, SendItem=Vec<u8>, RecvItem=Vec<u8>>,
    S: Spawn + Clone,
{
    #[allow(unused)]
    pub fn new(connector: C, spawner: S, timer_client: TimerClient, keepalive_ticks: usize) -> ClientConnector<C,S> {
        ClientConnector {
            connector,
            spawner,
            timer_client,
            keepalive_ticks,
        }
    }

    async fn relay_connect(&self, relay_address: A, remote_public_key: PublicKey) 
        -> Result<ConnPair<Vec<u8>,Vec<u8>>, ClientConnectorError> {

        let mut conn_pair = await!(self.connector.connect(relay_address))
            .ok_or(ClientConnectorError::InnerConnectorError)?;

        // Send an InitConnection::Connect(PublicKey) message to remote side:
        let init_connection = InitConnection::Connect(remote_public_key);
        let ser_init_connection = serialize_init_connection(&init_connection);
        await!(conn_pair.sender.send(ser_init_connection))
            .map_err(|_| ClientConnectorError::SendInitConnectionError)?;

        let ConnPair {sender, receiver} = conn_pair;

        // Deserialize receiver's messages:
        let from_tunnel_receiver = receiver.map(|vec| {
            deserialize_tunnel_message(&vec).ok()
        }).take_while(|opt_tunnel_message| {
            future::ready(opt_tunnel_message.is_some())
        }).map(|opt| opt.unwrap());

        // Serialize sender's messages:
        let to_tunnel_sender = sender
            .sink_map_err(|_| ())
            .with(|tunnel_message| -> future::Ready<Result<Vec<u8>, ()>> {
                future::ready(Ok(serialize_tunnel_message(&tunnel_message)))
            });

        let (user_from_tunnel_sender, user_from_tunnel) = mpsc::channel(0);
        let (user_to_tunnel, user_to_tunnel_receiver) = mpsc::channel(0);

        let mut c_timer_client = self.timer_client.clone();
        let timer_stream = await!(c_timer_client.request_timer_stream())
            .map_err(|_| ClientConnectorError::RequestTimerStreamError)?;

        let client_tunnel = client_tunnel(to_tunnel_sender, from_tunnel_receiver,
                                          user_from_tunnel_sender, user_to_tunnel_receiver,
                                          timer_stream,
                                          self.keepalive_ticks)
            .map_err(|e| {
                error!("client_tunnel error: {:?}", e);
            }).then(|_| future::ready(()));

        let mut c_spawner = self.spawner.clone();
        c_spawner.spawn(client_tunnel)
            .map_err(|_| ClientConnectorError::SpawnClientTunnelError)?;

        Ok(ConnPair {
            sender: user_to_tunnel,
            receiver: user_from_tunnel,
        })
    }
}

impl<A,C,S> Connector for ClientConnector<C,S> 
where
    A: Sync + Send + 'static,
    C: Connector<Address=A, SendItem=Vec<u8>, RecvItem=Vec<u8>> + Sync + Send,
    S: Spawn + Clone + Sync + Send,
{
    type Address = (A, PublicKey);
    type SendItem = Vec<u8>;
    type RecvItem = Vec<u8>;

    fn connect(&self, address: (A, PublicKey)) -> FutureObj<Option<ConnPair<Self::SendItem, Self::RecvItem>>> {
        let (relay_address, remote_public_key) = address;
        let relay_connect = self.relay_connect(relay_address, remote_public_key)
            .map(|res| res.ok());
        FutureObj::new(relay_connect.boxed())
    }
}



#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use std::marker::PhantomData;
    use futures::executor::ThreadPool;

    use timer::{create_timer_incoming};
    use crypto::identity::{PUBLIC_KEY_LEN};

    /// A connector that contains only one pre-created connection.
    struct DummyConnector<SI,RI,A> {
        // Note: Mutex is used here for interior mutability (of connect() trait method)
        mutex_opt_conn_pair: Mutex<Option<ConnPair<SI,RI>>>,
        phantom_a: PhantomData<A>
    }

    impl<SI,RI,A> DummyConnector<SI,RI,A> {
        fn new(conn_pair: ConnPair<SI,RI>) -> Self {
            DummyConnector { 
                mutex_opt_conn_pair: Mutex::new(Some(conn_pair)),
                phantom_a: PhantomData,
            }
        }
    }

    impl<SI,RI,A> Connector for DummyConnector<SI,RI,A> 
    where
        SI: Send,
        RI: Send,
    {
        type Address = A;
        type SendItem = SI;
        type RecvItem = RI;

        fn connect(&self, _address: A) -> FutureObj<Option<ConnPair<Self::SendItem, Self::RecvItem>>> {
            let mut guard = self.mutex_opt_conn_pair.lock().unwrap();
            let opt_conn_pair = guard.take();
            let future_obj = FutureObj::new(future::ready(opt_conn_pair).boxed());
            future_obj
            
        }
    }

    async fn task_client_connector_basic(spawner: impl Spawn + Clone + Sync + Send) {
        // Create a mock time service:
        let (mut tick_sender, tick_receiver) = mpsc::channel::<()>(0);
        let mut timer_client = create_timer_incoming(tick_receiver, spawner.clone()).unwrap();

        let keepalive_ticks = 16;
        let (local_sender, mut relay_receiver) = mpsc::channel::<Vec<u8>>(0);
        let (relay_sender, local_receiver) = mpsc::channel::<Vec<u8>>(0);

        let connector: DummyConnector<_,_,u32> = DummyConnector::new(ConnPair {
            sender: local_sender,
            receiver: local_receiver,
        });

        let client_connector = ClientConnector::new(
            connector,
            spawner.clone(),
            timer_client,
            keepalive_ticks);

        let address: u32 = 15;
        let public_key = PublicKey::from(&[0x77; PUBLIC_KEY_LEN]);
        let conn_pair = await!(client_connector.connect((address, public_key))).unwrap();

        await!(relay_receiver.next()).unwrap();
    }

    #[test]
    fn test_client_connector_basic() {
        let mut thread_pool = ThreadPool::new().unwrap();
        thread_pool.run(task_client_connector_basic(thread_pool.clone()));
    }

}

