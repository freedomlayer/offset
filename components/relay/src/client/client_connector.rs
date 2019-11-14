use futures::{FutureExt, SinkExt};

use common::conn::{BoxFuture, ConnPairVec, FutTransform};

use proto::crypto::PublicKey;
use proto::proto_ser::ProtoSerialize;
use proto::relay::messages::InitConnection;

#[derive(Debug)]
pub enum ClientConnectorError {
    InnerConnectorError,
    SendInitConnectionError,
}

/// ClientConnector is an end-to-end connector to a remote node.
/// It relies on a given connector C to a relay.
#[derive(Clone)]
pub struct ClientConnector<C, FT> {
    connector: C,
    keepalive_transform: FT,
}

impl<A, C, FT> ClientConnector<C, FT>
where
    A: 'static,
    C: FutTransform<Input = A, Output = Option<ConnPairVec>>,
    FT: FutTransform<Input = ConnPairVec, Output = ConnPairVec>,
{
    pub fn new(connector: C, keepalive_transform: FT) -> ClientConnector<C, FT> {
        ClientConnector {
            connector,
            keepalive_transform,
        }
    }

    async fn relay_connect(
        &mut self,
        relay_address: A,
        remote_public_key: PublicKey,
    ) -> Result<ConnPairVec, ClientConnectorError> {
        let (mut sender, receiver) = self
            .connector
            .transform(relay_address)
            .await
            .ok_or(ClientConnectorError::InnerConnectorError)?.split();

        // Send an InitConnection::Connect(PublicKey) message to remote side:
        let init_connection = InitConnection::Connect(remote_public_key);
        let ser_init_connection = init_connection.proto_serialize();
        sender
            .send(ser_init_connection)
            .await
            .map_err(|_| ClientConnectorError::SendInitConnectionError)?;

        let from_tunnel_receiver = receiver;
        let to_tunnel_sender = sender;

        // TODO: Do something about the unwrap here:
        // Maybe change ConnTransform trait to allow force returning something that is not None?
        let (user_to_tunnel, user_from_tunnel) = self
            .keepalive_transform
            .transform(ConnPairVec::from_raw(to_tunnel_sender, from_tunnel_receiver))
            .await.split();

        Ok(ConnPairVec::from_raw(user_to_tunnel, user_from_tunnel))
    }
}

impl<A, C, FT> FutTransform for ClientConnector<C, FT>
where
    A: Sync + Send + 'static,
    C: FutTransform<Input = A, Output = Option<ConnPairVec>> + Send + Sync,
    FT: FutTransform<Input = ConnPairVec, Output = ConnPairVec> + Send,
{
    type Input = (A, PublicKey);
    type Output = Option<ConnPairVec>;

    fn transform(&mut self, input: (A, PublicKey)) -> BoxFuture<'_, Self::Output> {
        let (relay_address, remote_public_key) = input;
        let relay_connect = self
            .relay_connect(relay_address, remote_public_key)
            .map(Result::ok);
        Box::pin(relay_connect)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::channel::mpsc;
    use futures::executor::{ThreadPool, LocalPool};
    use futures::task::{Spawn, SpawnExt};
    use futures::{future, StreamExt};

    use proto::proto_ser::ProtoDeserialize;

    use common::conn::FuncFutTransform;
    use common::dummy_connector::DummyConnector;

    async fn task_client_connector_basic(spawner: impl Spawn + Clone + Sync + Send + 'static) {
        let (local_sender, mut relay_receiver) = mpsc::channel::<Vec<u8>>(1);
        let (mut relay_sender, local_receiver) = mpsc::channel::<Vec<u8>>(1);

        let conn_pair = ConnPairVec::from_raw(local_sender, local_receiver);
        let (req_sender, mut req_receiver) = mpsc::channel(1);
        // conn_sender.send(conn_pair).await.unwrap();
        let connector = DummyConnector::new(req_sender);

        // keepalive_transform does nothing:
        let keepalive_transform = FuncFutTransform::new(|x| Box::pin(future::ready(x)));

        let mut client_connector = ClientConnector::new(connector, keepalive_transform);

        let address: u32 = 15;
        let public_key = PublicKey::from(&[0x77; PublicKey::len()]);
        let c_public_key = public_key.clone();
        let fut_conn_pair = spawner
            .spawn_with_handle(async move {
                client_connector
                    .transform((address, c_public_key))
                    .await
                    .unwrap()
            })
            .unwrap();

        // Wait for connection request:
        let req = req_receiver.next().await.unwrap();
        // Reply with a connection:
        req.reply(Some(conn_pair));
        let conn_pair = fut_conn_pair.await;

        let vec = relay_receiver.next().await.unwrap();
        let init_connection = InitConnection::proto_deserialize(&vec).unwrap();
        match init_connection {
            InitConnection::Connect(conn_public_key) => assert_eq!(conn_public_key, public_key),
            _ => unreachable!(),
        };

        relay_sender.send(vec![1, 2, 3]).await.unwrap();
        let (ref _sender, ref mut receiver) = conn_pair.split();
        let vec = receiver.next().await.unwrap();
        assert_eq!(vec, vec![1, 2, 3]);
    }

    #[test]
    fn test_client_connector_basic() {
        let thread_pool = ThreadPool::new().unwrap();
        LocalPool::new().run_until(task_client_connector_basic(thread_pool.clone()));
    }
}
