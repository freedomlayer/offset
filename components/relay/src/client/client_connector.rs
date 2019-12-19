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
pub struct ClientConnector<C> {
    connector: C,
}

impl<A, C> ClientConnector<C>
where
    A: 'static,
    C: FutTransform<Input = A, Output = Option<ConnPairVec>>,
{
    pub fn new(connector: C) -> ClientConnector<C> {
        ClientConnector {
            connector,
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
            .ok_or(ClientConnectorError::InnerConnectorError)?
            .split();

        // Send an InitConnection::Connect(PublicKey) message to remote side:
        let init_connection = InitConnection::Connect(remote_public_key);
        let ser_init_connection = init_connection.proto_serialize();
        sender
            .send(ser_init_connection)
            .await
            .map_err(|_| ClientConnectorError::SendInitConnectionError)?;

        Ok(ConnPairVec::from_raw(sender, receiver))
    }
}

impl<A, C> FutTransform for ClientConnector<C>
where
    A: Send + 'static,
    C: FutTransform<Input = A, Output = Option<ConnPairVec>> + Send,
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
    use futures::executor::{LocalPool, ThreadPool};
    use futures::task::{Spawn, SpawnExt};
    use futures::StreamExt;

    use proto::proto_ser::ProtoDeserialize;

    use common::dummy_connector::DummyConnector;

    async fn task_client_connector_basic(spawner: impl Spawn + Clone + Send + 'static) {
        let (local_sender, mut relay_receiver) = mpsc::channel::<Vec<u8>>(1);
        let (mut relay_sender, local_receiver) = mpsc::channel::<Vec<u8>>(1);

        let conn_pair = ConnPairVec::from_raw(local_sender, local_receiver);
        let (req_sender, mut req_receiver) = mpsc::channel(1);
        // conn_sender.send(conn_pair).await.unwrap();
        let connector = DummyConnector::new(req_sender);

        let mut client_connector = ClientConnector::new(connector);

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
