use std::collections::HashMap;
use std::marker::Unpin;

use futures::channel::oneshot;
use futures::{future, stream, Sink, SinkExt, Stream, StreamExt};

use common::conn::{BoxStream, ConnPair};
use common::select_streams::select_streams;

use proto::crypto::{HashResult, PublicKey, RandValue, Signature, Uid};

use proto::index_server::messages::{
    IndexClientToServer, IndexMutation, IndexServerToClient, MultiRoute, MutationsUpdate,
    RequestRoutes, ResponseRoutes,
};

use signature::signature_buff::create_mutations_update_signature_buff;

use crypto::rand::{CryptoRandom, RandGen};

use identity::IdentityClient;

pub type ServerConn = ConnPair<IndexClientToServer, IndexServerToClient>;

#[derive(Debug)]
pub enum SingleClientControl {
    RequestRoutes((RequestRoutes, oneshot::Sender<Vec<MultiRoute>>)),
    SendMutations(Vec<IndexMutation>),
}

#[derive(Debug, PartialEq, Eq)]
pub enum SingleClientError {
    ControlClosed,
    ServerClosed,
    SendToServerError,
    RequestSignatureFailed,
    CounterOverflow,
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
enum SingleClientEvent {
    FromServer(IndexServerToClient),
    ServerClosed,
    Control(SingleClientControl),
    ControlClosed,
}

struct SingleClient<TS, R> {
    local_public_key: PublicKey,
    identity_client: IdentityClient,
    rng: R,
    to_server: TS,
    session_id: Uid,
    counter: u64,
    /// Last time_hash sent by the server
    /// We use this value to prove that our signatures are recent
    server_time_hash: HashResult,
    /// Unanswered requests, waiting for a response from the server
    open_requests: HashMap<Uid, oneshot::Sender<Vec<MultiRoute>>>,
}

impl<TS, R> SingleClient<TS, R>
where
    TS: Sink<IndexClientToServer> + Unpin,
    R: CryptoRandom,
{
    pub fn new(
        local_public_key: PublicKey,
        identity_client: IdentityClient,
        rng: R,
        to_server: TS,
        session_id: Uid,
        server_time_hash: HashResult,
    ) -> Self {
        SingleClient {
            local_public_key,
            identity_client,
            rng,
            to_server,
            session_id,
            counter: 0,
            server_time_hash,
            open_requests: HashMap::new(),
        }
    }

    /// Handle a message coming from the server
    pub async fn handle_server_message(
        &mut self,
        index_server_to_client: IndexServerToClient,
    ) -> Result<(), SingleClientError> {
        match index_server_to_client {
            IndexServerToClient::TimeHash(time_hash) => self.server_time_hash = time_hash,
            IndexServerToClient::ResponseRoutes(response_routes) => {
                let ResponseRoutes {
                    request_id,
                    multi_routes,
                } = response_routes;
                let request_sender = match self.open_requests.remove(&request_id) {
                    Some(request_sender) => request_sender,
                    None => {
                        warn!(
                            "Received a response for unrecognized request_id: {:?}",
                            &request_id
                        );
                        return Ok(());
                    }
                };
                if request_sender.send(multi_routes).is_err() {
                    warn!(
                        "Failed to return response for request_id: {:?} ",
                        &request_id
                    );
                }
            }
        }
        Ok(())
    }

    /// Handle a control message (from the IndexClient code)
    pub async fn handle_control_message(
        &mut self,
        single_client_control: SingleClientControl,
    ) -> Result<(), SingleClientError> {
        match single_client_control {
            SingleClientControl::RequestRoutes((request_routes, response_sender)) => {
                // Add a new open request:
                self.open_requests
                    .insert(request_routes.request_id.clone(), response_sender);

                // Send request to server:
                let to_server_message = IndexClientToServer::RequestRoutes(request_routes);
                self.to_server
                    .send(to_server_message)
                    .await
                    .map_err(|_| SingleClientError::SendToServerError)?;
            }
            SingleClientControl::SendMutations(index_mutations) => {
                let mut mutations_update = MutationsUpdate {
                    node_public_key: self.local_public_key.clone(),
                    index_mutations,
                    time_hash: self.server_time_hash.clone(),
                    session_id: self.session_id.clone(),
                    counter: self.counter,
                    rand_nonce: RandValue::rand_gen(&self.rng),
                    signature: Signature::default(),
                };

                // Calculate signature:
                mutations_update.signature = self
                    .identity_client
                    .request_signature(create_mutations_update_signature_buff(&mutations_update))
                    .await
                    .map_err(|_| SingleClientError::RequestSignatureFailed)?;

                // Advance counter:
                // We assume that the counter will never reach the wrapping point, because it is of
                // size at least 64 bits, but we still add an error here, just in case.
                self.counter = self
                    .counter
                    .checked_add(1)
                    .ok_or(SingleClientError::CounterOverflow)?;

                // Send MutationsUpdate to server:
                let to_server_message = IndexClientToServer::MutationsUpdate(mutations_update);
                self.to_server
                    .send(to_server_message)
                    .await
                    .map_err(|_| SingleClientError::SendToServerError)?;
            }
        }
        Ok(())
    }
}

/// Wait for the first time hash sent from the server.
pub async fn first_server_time_hash(
    from_server: &mut BoxStream<'static, IndexServerToClient>,
) -> Result<HashResult, SingleClientError> {
    loop {
        match from_server.next().await {
            None => return Err(SingleClientError::ServerClosed),
            Some(IndexServerToClient::TimeHash(time_hash)) => return Ok(time_hash),
            Some(index_server_to_client) => warn!(
                "first_server_time_hash(): Received message {:?} before first time has",
                index_server_to_client
            ),
        }
    }
}

pub async fn single_client_loop<IC, R>(
    server_conn: ServerConn,
    incoming_control: IC,
    local_public_key: PublicKey,
    identity_client: IdentityClient,
    rng: R,
    first_server_time_hash: HashResult,
) -> Result<(), SingleClientError>
where
    IC: Stream<Item = SingleClientControl> + Send + Unpin,
    R: CryptoRandom,
{
    let (to_server, from_server) = server_conn.split();

    let session_id = Uid::rand_gen(&rng);
    let mut single_client = SingleClient::new(
        local_public_key,
        identity_client,
        rng,
        to_server,
        session_id,
        first_server_time_hash,
    );

    let from_server = from_server
        .map(SingleClientEvent::FromServer)
        .chain(stream::once(future::ready(SingleClientEvent::ServerClosed)));

    let incoming_control = incoming_control
        .map(SingleClientEvent::Control)
        .chain(stream::once(future::ready(
            SingleClientEvent::ControlClosed,
        )));

    let mut events = select_streams![from_server, incoming_control];

    while let Some(event) = events.next().await {
        match event {
            SingleClientEvent::FromServer(index_server_to_client) => {
                single_client
                    .handle_server_message(index_server_to_client)
                    .await?
            }
            SingleClientEvent::ServerClosed => return Err(SingleClientError::ServerClosed),
            SingleClientEvent::Control(index_client_control) => {
                single_client
                    .handle_control_message(index_client_control)
                    .await?
            }
            SingleClientEvent::ControlClosed => return Err(SingleClientError::ControlClosed),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::convert::TryFrom;

    use futures::channel::{mpsc, oneshot};
    use futures::executor::{block_on, ThreadPool};
    use futures::future::join;
    use futures::task::{Spawn, SpawnExt};
    use futures::{FutureExt, TryFutureExt};

    use proto::crypto::PrivateKey;
    use proto::funder::messages::Currency;

    use signature::verify::verify_mutations_update;

    use crypto::identity::{Identity, SoftwareEd25519Identity};
    use crypto::rand::RandGen;
    use crypto::test_utils::DummyRandom;

    use identity::create_identity;

    async fn task_first_server_time_hash() {
        let (mut to_server, from_server) = mpsc::channel(0);
        let time_hash = HashResult::from(&[1; HashResult::len()]);

        let fut_send = to_server.send(IndexServerToClient::TimeHash(time_hash.clone()));
        let mut from_server_boxed = from_server.boxed();
        let fut_time_hash = first_server_time_hash(&mut from_server_boxed);

        let (_, res_time_hash) = join(fut_send, fut_time_hash).await;
        assert_eq!(res_time_hash.unwrap(), time_hash);
    }

    #[test]
    fn test_first_server_time_hash() {
        block_on(task_first_server_time_hash());
    }

    async fn task_first_server_time_hash_server_closed() {
        let (to_server, from_server) = mpsc::channel(0);
        // Simulate closing the connection to the server:
        drop(to_server);

        let mut from_server_boxed = from_server.boxed();
        let fut_time_hash = first_server_time_hash(&mut from_server_boxed);

        let res_time_hash = fut_time_hash.await;
        assert_eq!(res_time_hash, Err(SingleClientError::ServerClosed));
    }

    #[test]
    fn test_first_server_time_hash_server_closed() {
        block_on(task_first_server_time_hash_server_closed());
    }

    async fn task_single_client_loop_basic<S>(spawner: S)
    where
        S: Spawn,
    {
        let currency = Currency::try_from("FST".to_owned()).unwrap();
        let (mut server_sender, client_receiver) = mpsc::channel(0);
        let (client_sender, mut server_receiver) = mpsc::channel(0);
        let (mut control_sender, incoming_control) = mpsc::channel(0);

        // Create identity_client:
        let rng = DummyRandom::new(&[1u8]);
        let pkcs8 = PrivateKey::rand_gen(&rng);
        let identity = SoftwareEd25519Identity::from_private_key(&pkcs8).unwrap();
        let local_public_key = identity.get_public_key();
        let (requests_sender, identity_server) = create_identity(identity);
        spawner.spawn(identity_server.map(|_| ())).unwrap();
        let identity_client = IdentityClient::new(requests_sender);

        let server_conn = ConnPair::from_raw(client_sender, client_receiver);
        let rng = DummyRandom::new(&[2u8]);
        let first_server_time_hash = HashResult::from(&[1; HashResult::len()]);

        let loop_fut = single_client_loop(
            server_conn,
            incoming_control,
            local_public_key.clone(),
            identity_client,
            rng,
            first_server_time_hash,
        )
        .map_err(|e| error!("single_client_loop() error: {:?}", e))
        .map(|_| ());

        spawner.spawn(loop_fut).unwrap();

        // Send some time hashes from server:
        for i in 2..8 {
            let time_hash = HashResult::from(&[i; HashResult::len()]);
            server_sender
                .send(IndexServerToClient::TimeHash(time_hash))
                .await
                .unwrap();
        }

        // Request routes:
        let request_routes = RequestRoutes {
            request_id: Uid::from(&[3; Uid::len()]),
            currency: currency.clone(),
            capacity: 20,
            source: PublicKey::from(&[0xcc; PublicKey::len()]),
            destination: PublicKey::from(&[0xdd; PublicKey::len()]),
            opt_exclude: None,
        };

        let (response_sender, response_receiver) = oneshot::channel();
        let single_client_control =
            SingleClientControl::RequestRoutes((request_routes.clone(), response_sender));

        control_sender.send(single_client_control).await.unwrap();

        // Request routes is redirected to server:
        let index_client_to_server = server_receiver.next().await.unwrap();
        match index_client_to_server {
            IndexClientToServer::RequestRoutes(sent_request_routes) => {
                assert_eq!(request_routes, sent_request_routes);
            }
            _ => unreachable!(),
        };

        // Server sends response routes:
        let response_routes = ResponseRoutes {
            request_id: Uid::from(&[3; Uid::len()]),
            multi_routes: vec![], // No suitable routes were found
        };

        server_sender
            .send(IndexServerToClient::ResponseRoutes(response_routes.clone()))
            .await
            .unwrap();

        // Client receives response routes:
        let multi_routes = response_receiver.await.unwrap();
        assert_eq!(multi_routes, vec![]);

        for iter in 0..3 {
            // Counter should increment every time
            // Send mutations:
            control_sender
                .send(SingleClientControl::SendMutations(vec![]))
                .await
                .unwrap();

            // Mutations should be sent to the server:
            let index_client_to_server = server_receiver.next().await.unwrap();
            match index_client_to_server {
                IndexClientToServer::MutationsUpdate(mutations_update) => {
                    assert_eq!(mutations_update.node_public_key, local_public_key);
                    assert_eq!(mutations_update.index_mutations, vec![]);
                    // Counter should match iter:
                    assert_eq!(mutations_update.counter, iter);
                    assert_eq!(
                        mutations_update.time_hash,
                        HashResult::from(&[7; HashResult::len()])
                    );

                    assert!(verify_mutations_update(&mutations_update));
                }
                _ => unreachable!(),
            };
        }
    }

    #[test]
    fn test_single_client_loop_basic() {
        let thread_pool = ThreadPool::new().unwrap();
        block_on(task_single_client_loop_basic(thread_pool.clone()));
    }
}
