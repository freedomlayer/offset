use crypto::identity::PublicKey;
use futures::{FutureExt, SinkExt};

use common::conn::{BoxFuture, Connector, 
    ConnPair, FutTransform};

use proto::relay::messages::{InitConnection};
use proto::relay::serialize::serialize_init_connection;


#[derive(Debug)]
pub enum ClientConnectorError {
    InnerConnectorError,
    SendInitConnectionError,
    RequestTimerStreamError,
    SpawnClientTunnelError,
}

/// ClientConnector is an end-to-end connector to a remote node.
/// It relies on a given connector C to a relay.
#[derive(Clone)]
pub struct ClientConnector<C,FT> {
    connector: C,
    keepalive_transform: FT,
}

impl<A: 'static,C,FT> ClientConnector<C,FT> 
where
    C: Connector<Address=A, SendItem=Vec<u8>, RecvItem=Vec<u8>>,
    FT: FutTransform<Input=ConnPair<Vec<u8>,Vec<u8>>,
                     Output=ConnPair<Vec<u8>,Vec<u8>>>,
{
    pub fn new(connector: C, 
               keepalive_transform: FT) -> ClientConnector<C,FT> {

        ClientConnector {
            connector,
            keepalive_transform,
        }
    }

    async fn relay_connect(&mut self, relay_address: A, remote_public_key: PublicKey) 
        -> Result<ConnPair<Vec<u8>,Vec<u8>>, ClientConnectorError> {

        let (mut sender, receiver) = await!(self.connector.connect(relay_address))
            .ok_or(ClientConnectorError::InnerConnectorError)?;

        // Send an InitConnection::Connect(PublicKey) message to remote side:
        let init_connection = InitConnection::Connect(remote_public_key);
        let ser_init_connection = serialize_init_connection(&init_connection);
        await!(sender.send(ser_init_connection))
            .map_err(|_| ClientConnectorError::SendInitConnectionError)?;

        let from_tunnel_receiver = receiver;
        let to_tunnel_sender = sender;

        // TODO; Do something about the unwrap here:
        // Maybe change ConnTransform trait to allow force returning something that is not None?
        let (user_to_tunnel, user_from_tunnel) = 
            await!(self.keepalive_transform.transform((to_tunnel_sender, from_tunnel_receiver)));

        Ok((user_to_tunnel, user_from_tunnel))
    }
}

impl<A,C,FT> Connector for ClientConnector<C,FT> 
where
    A: Sync + Send + 'static,
    C: Connector<Address=A, SendItem=Vec<u8>, RecvItem=Vec<u8>> + Sync + Send,
    FT: FutTransform<Input=ConnPair<Vec<u8>,Vec<u8>>,
                     Output=ConnPair<Vec<u8>,Vec<u8>>> + Send,
{
    type Address = (A, PublicKey);
    type SendItem = Vec<u8>;
    type RecvItem = Vec<u8>;

    fn connect(&mut self, address: (A, PublicKey)) 
        -> BoxFuture<'_, Option<ConnPair<Self::SendItem, Self::RecvItem>>> {

        let (relay_address, remote_public_key) = address;
        let relay_connect = self.relay_connect(relay_address, remote_public_key)
            .map(|res| res.ok());
        Box::pinned(relay_connect)
    }
}



#[cfg(test)]
mod tests {
    use super::*;
    use futures::executor::ThreadPool;
    use futures::channel::mpsc;
    use futures::task::{Spawn, SpawnExt};
    use futures::StreamExt;

    use crypto::identity::{PUBLIC_KEY_LEN};
    use proto::relay::serialize::deserialize_init_connection;

    use common::dummy_connector::DummyConnector;
    use common::conn::FuncFutTransform;

    async fn task_client_connector_basic(mut spawner: impl Spawn + Clone + Sync + Send + 'static) {

        let (local_sender, mut relay_receiver) = mpsc::channel::<Vec<u8>>(0);
        let (mut relay_sender, local_receiver) = mpsc::channel::<Vec<u8>>(0);

        let conn_pair = (local_sender, local_receiver);
        let (req_sender, mut req_receiver) = mpsc::channel(0);
        // await!(conn_sender.send(conn_pair)).unwrap();
        let connector = DummyConnector::new(req_sender);

        // keepalive_transform does nothing:
        let keepalive_transform = FuncFutTransform::new(|x| x);

        let mut client_connector = ClientConnector::new(
            connector,
            keepalive_transform);

        let address: u32 = 15;
        let public_key = PublicKey::from(&[0x77; PUBLIC_KEY_LEN]);
        let c_public_key = public_key.clone();
        let fut_conn_pair = spawner.spawn_with_handle(async move {
            await!(client_connector.connect((address, c_public_key))).unwrap()
        }).unwrap();

        // Wait for connection request:
        let req = await!(req_receiver.next()).unwrap();
        // Reply with a connection:
        req.reply(Some(conn_pair));
        let mut conn_pair = await!(fut_conn_pair);

        let vec = await!(relay_receiver.next()).unwrap();
        let init_connection = deserialize_init_connection(&vec).unwrap();
        match init_connection {
            InitConnection::Connect(conn_public_key) => assert_eq!(conn_public_key, public_key),
            _ => unreachable!(),
        };

        await!(relay_sender.send(vec![1,2,3])).unwrap();
        let (ref _sender, ref mut receiver) = conn_pair;
        let vec = await!(receiver.next()).unwrap();
        assert_eq!(vec, vec![1,2,3]);
    }

    #[test]
    fn test_client_connector_basic() {
        let mut thread_pool = ThreadPool::new().unwrap();
        thread_pool.run(task_client_connector_basic(thread_pool.clone()));
    }
}

