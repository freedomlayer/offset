use futures::channel::{mpsc, oneshot};
use futures::task::{Spawn, SpawnExt};
use futures::FutureExt;

use crypto::rand::CryptoRandom;
use crypto::identity::PublicKey;

use identity::IdentityClient;

use common::conn::{BoxFuture, FutTransform};

use crate::single_client::{
    first_server_time_hash, single_client_loop, ServerConn, SingleClientControl, SingleClientError,
};

pub type ControlSender = mpsc::Sender<SingleClientControl>;
pub type CloseReceiver = oneshot::Receiver<Result<(), SingleClientError>>;
pub type SessionHandle = (ControlSender, CloseReceiver);

#[derive(Clone)]
pub struct IndexClientSession<C, R, S> {
    connector: C,
    local_public_key: PublicKey,
    identity_client: IdentityClient,
    rng: R,
    spawner: S,
}

impl<ISA, C, R, S> IndexClientSession<C, R, S>
where
    ISA: Send + 'static,
    C: FutTransform<Input = ISA, Output = Option<ServerConn>> + Send,
    S: Spawn + Send,
    R: CryptoRandom + Clone + 'static,
{
    #[allow(unused)]
    pub fn new(
        connector: C,
        local_public_key: PublicKey,
        identity_client: IdentityClient,
        rng: R,
        spawner: S,
    ) -> Self {
        IndexClientSession {
            connector,
            local_public_key,
            identity_client,
            rng,
            spawner,
        }
    }

    async fn connect(&mut self, index_server_address: ISA) -> Option<SessionHandle> {
        let (to_server, mut from_server) = await!(self.connector.transform(index_server_address))?;

        let first_time_hash = await!(first_server_time_hash(&mut from_server)).ok()?;
        let (control_sender, incoming_control) = mpsc::channel(0);

        let (close_sender, close_receiver) = oneshot::channel();

        let single_client_fut = single_client_loop(
            (to_server, from_server),
            incoming_control,
            self.local_public_key.clone(),
            self.identity_client.clone(),
            self.rng.clone(),
            first_time_hash,
        )
        .map(|res| {
            if let Err(res) = close_sender.send(res) {
                error!("Failed to send result from single_client_loop(): {:?}", res);
            }
        });

        self.spawner.spawn(single_client_fut).ok()?;
        Some((control_sender, close_receiver))
    }
}

impl<ISA, C, R, S> FutTransform for IndexClientSession<C, R, S>
where
    ISA: Send + 'static,
    C: FutTransform<Input = ISA, Output = Option<ServerConn>> + Send,
    S: Spawn + Send,
    R: CryptoRandom + Clone + 'static,
{
    /// Address of an index server
    type Input = ISA;
    /// A pair: control sender, and a receiver that notifies about disconnection.
    type Output = Option<SessionHandle>;

    fn transform(&mut self, index_server_address: Self::Input) -> BoxFuture<'_, Self::Output> {
        Box::pin(self.connect(index_server_address))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use futures::executor::ThreadPool;
    use futures::future::join;
    use futures::task::{Spawn, SpawnExt};
    use futures::{SinkExt, StreamExt};

    use crypto::hash::{HashResult, HASH_RESULT_LEN};
    use crypto::identity::{generate_pkcs8_key_pair, Identity, SoftwareEd25519Identity};
    use crypto::test_utils::DummyRandom;

    use identity::create_identity;

    use common::dummy_connector::DummyConnector;
    use proto::index_server::messages::IndexServerToClient;

    async fn task_index_client_session_basic<S>(mut spawner: S)
    where
        S: Spawn + Clone + Send,
    {
        // Create identity_client:
        let rng = DummyRandom::new(&[1u8]);
        let pkcs8 = generate_pkcs8_key_pair(&rng);
        let identity = SoftwareEd25519Identity::from_pkcs8(&pkcs8).unwrap();
        let local_public_key = identity.get_public_key();
        let (requests_sender, identity_server) = create_identity(identity);
        spawner.spawn(identity_server.map(|_| ())).unwrap();
        let identity_client = IdentityClient::new(requests_sender);

        let (connector_sender, mut connector_receiver) = mpsc::channel(0);
        let connector = DummyConnector::<u32, _>::new(connector_sender);

        let mut index_client_session = IndexClientSession::new(
            connector,
            local_public_key,
            identity_client,
            rng,
            spawner.clone(),
        );

        let (server_sender, client_receiver) = mpsc::channel(0);
        let (client_sender, _server_receiver) = mpsc::channel(0);

        let mut c_server_sender = server_sender.clone();

        let handle_conn_request_fut = async move {
            let conn_request = await!(connector_receiver.next()).unwrap();
            conn_request.reply(Some((client_sender, client_receiver)));

            // Send a first time hash (Required for connection):
            let time_hash = HashResult::from(&[0xaa; HASH_RESULT_LEN]);
            await!(c_server_sender.send(IndexServerToClient::TimeHash(time_hash))).unwrap();
        };
        let session_handle_fut = index_client_session.transform(0x1337u32);

        let (opt_session_handle, ()) = await!(join(session_handle_fut, handle_conn_request_fut));
        let (_control_sender, close_receiver) = opt_session_handle.unwrap();

        drop(server_sender);
        let single_client_loop_res = await!(close_receiver).unwrap();
        assert_eq!(single_client_loop_res, Err(SingleClientError::ServerClosed));
    }

    #[test]
    fn test_index_client_session_basic() {
        let mut thread_pool = ThreadPool::new().unwrap();
        thread_pool.run(task_index_client_session_basic(thread_pool.clone()));
    }
}
