use std::marker::Unpin;
use std::collections::HashMap;


use futures::{future, TryFutureExt, FutureExt, stream, Stream, StreamExt, Sink, SinkExt};
use futures::channel::{oneshot, mpsc};

use common::conn::ConnPair;
use crypto::crypto_rand::{RandValue, CryptoRandom};
use crypto::uid::Uid;
use crypto::hash::HashResult;
use crypto::identity::{PublicKey, Signature};

use identity::IdentityClient;

use proto::index_server::messages::{IndexServerToClient, 
    IndexClientToServer, 
    ResponseRoutes, RouteWithCapacity, MutationsUpdate,
    IndexMutation,
    RequestRoutes};

pub type ServerConn = ConnPair<IndexClientToServer, IndexServerToClient>;

#[derive(Debug)]
pub enum SingleClientControl {
    RequestRoutes((RequestRoutes, oneshot::Sender<Vec<RouteWithCapacity>>)),
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

#[derive(Debug)]
enum SingleClientEvent {
    FromServer(IndexServerToClient),
    ServerClosed,
    Control(SingleClientControl),
    ControlClosed,
}

struct SingleClient<TS,R> {
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
    open_requests: HashMap<Uid, oneshot::Sender<Vec<RouteWithCapacity>>>,
}

impl<TS,R> SingleClient<TS,R> 
where
    TS: Sink<SinkItem=IndexClientToServer> + Unpin,
    R: CryptoRandom,
{
    pub fn new(local_public_key: PublicKey, 
               identity_client: IdentityClient, 
               rng: R,
               to_server: TS,
               session_id: Uid,
               server_time_hash: HashResult) -> Self {

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
    pub async fn handle_server_message(&mut self,
                                       index_server_to_client: IndexServerToClient) 
        -> Result<(), SingleClientError> {

        match index_server_to_client {
            IndexServerToClient::TimeHash(time_hash) => self.server_time_hash = time_hash,
            IndexServerToClient::ResponseRoutes(response_routes) => {
                let ResponseRoutes { request_id, routes } = response_routes;
                let request_sender = match self.open_requests.remove(&request_id) {
                    Some(request_sender) => request_sender,
                    None => {
                        warn!("Received a response for unrecognized request_id: {:?}", &request_id);
                        return Ok(());
                    },
                };
                if let Err(_) = request_sender.send(routes) {
                    warn!("Failed to return response for request_id: {:?} ", &request_id);
                }
            },
        }
        Ok(())
    }

    /// Handle a control message (from the IndexClient code)
    pub async fn handle_control_message(&mut self, 
                                        single_client_control: SingleClientControl)
        -> Result<(), SingleClientError> {

        match single_client_control {
            SingleClientControl::RequestRoutes((request_routes, response_sender)) => {
                // Add a new open request:
                self.open_requests.insert(request_routes.request_id.clone(), response_sender);

                // Send request to server:
                let to_server_message = IndexClientToServer::RequestRoutes(request_routes);
                await!(self.to_server.send(to_server_message))
                    .map_err(|_| SingleClientError::SendToServerError)?;
            },
            SingleClientControl::SendMutations(index_mutations) => {
                let mut mutations_update = MutationsUpdate {
                    node_public_key: self.local_public_key.clone(),
                    index_mutations,
                    time_hash: self.server_time_hash.clone(),
                    session_id: self.session_id.clone(),
                    counter: self.counter,
                    rand_nonce: RandValue::new(&self.rng),
                    signature: Signature::zero(),
                };

                // Calculate signature:
                mutations_update.signature = await!(
                    self.identity_client.request_signature(mutations_update.signature_buff()))
                    .map_err(|_| SingleClientError::RequestSignatureFailed)?;

                // Advance counter:
                // We assume that the counter will never reach the wrapping point, because it is of
                // size at least 64 bits, but we still add an error here, just in case.
                self.counter = self.counter.checked_add(1)
                    .ok_or(SingleClientError::CounterOverflow)?;

                // Send MutationsUpdate to server:
                let to_server_message = IndexClientToServer::MutationsUpdate(mutations_update);
                await!(self.to_server.send(to_server_message))
                    .map_err(|_| SingleClientError::SendToServerError)?;
            },
        }
        Ok(())
    }
}

/// Wait for the first time hash sent from the server.
pub async fn first_server_time_hash(from_server: &mut mpsc::Receiver<IndexServerToClient>)
    -> Result<HashResult, SingleClientError> {

    loop {
        match await!(from_server.next()) {
            None => return Err(SingleClientError::ServerClosed),
            Some(IndexServerToClient::TimeHash(time_hash)) => return Ok(time_hash),
            Some(index_server_to_client) => 
                warn!("single_client_loop(): Received message {:?} before first time has", 
                      index_server_to_client),
        }
    }
}

pub async fn single_client_loop<IC,R>(server_conn: ServerConn,
                     incoming_control: IC,
                     local_public_key: PublicKey,
                     identity_client: IdentityClient,
                     rng: R,
                     first_server_time_hash: HashResult) -> Result<(), SingleClientError>
where
    IC: Stream<Item=SingleClientControl> + Unpin,
    R: CryptoRandom,
{
    let (to_server, from_server) = server_conn;

    let session_id = Uid::new(&rng);
    let mut single_client = SingleClient::new(local_public_key, 
                                              identity_client,
                                              rng,
                                              to_server,
                                              session_id,
                                              first_server_time_hash);

    let from_server = from_server
        .map(|index_server_to_client| SingleClientEvent::FromServer(index_server_to_client))
        .chain(stream::once(future::ready(SingleClientEvent::ServerClosed)));

    let incoming_control = incoming_control
        .map(|single_client_control| SingleClientEvent::Control(single_client_control))
        .chain(stream::once(future::ready(SingleClientEvent::ControlClosed)));

    let mut events = from_server.select(incoming_control);

    while let Some(event) = await!(events.next()) {
        match event {
            SingleClientEvent::FromServer(index_server_to_client) => 
                await!(single_client.handle_server_message(index_server_to_client))?,
            SingleClientEvent::ServerClosed => return Err(SingleClientError::ServerClosed),
            SingleClientEvent::Control(index_client_control) => 
                await!(single_client.handle_control_message(index_client_control))?,
            SingleClientEvent::ControlClosed => return Err(SingleClientError::ControlClosed),
        }
    }
    Ok(())
}


#[cfg(test)]
mod tests {
    use super::*;
    use crypto::hash::HASH_RESULT_LEN;
    use futures::executor::ThreadPool;
    use futures::task::{Spawn, SpawnExt};

    use crypto::test_utils::DummyRandom;
    use crypto::identity::{SoftwareEd25519Identity, 
        generate_pkcs8_key_pair, compare_public_key, Identity,
        PUBLIC_KEY_LEN};
    use crypto::crypto_rand::{CryptoRandom};
    use crypto::uid::{UID_LEN};

    use identity::create_identity;

    async fn task_first_server_time_hash() {
        let (mut to_server, mut from_server) = mpsc::channel(0);
        let time_hash = HashResult::from(&[1; HASH_RESULT_LEN]);

        let fut_send = to_server.send(IndexServerToClient::TimeHash(time_hash.clone()));
        let fut_time_hash = first_server_time_hash(&mut from_server);
        
        let (_, res_time_hash) = await!(fut_send.join(fut_time_hash));
        assert_eq!(res_time_hash.unwrap(), time_hash);
    }

    #[test]
    fn test_first_server_time_hash() {
        let mut thread_pool = ThreadPool::new().unwrap();
        thread_pool.run(task_first_server_time_hash());
    }

    async fn task_first_server_time_hash_server_closed() {
        let (to_server, mut from_server) = mpsc::channel(0);
        drop(to_server);

        let fut_time_hash = first_server_time_hash(&mut from_server);
        
        let res_time_hash = await!(fut_time_hash);
        assert_eq!(res_time_hash, Err(SingleClientError::ServerClosed));
    }

    #[test]
    fn test_first_server_time_hash_server_closed() {
        let mut thread_pool = ThreadPool::new().unwrap();
        thread_pool.run(task_first_server_time_hash_server_closed());
    }

    async fn task_single_client_loop_basic<S>(mut spawner: S) 
    where
        S: Spawn,
    {
        let (mut server_sender, client_receiver) = mpsc::channel(0);
        let (client_sender, mut server_receiver) = mpsc::channel(0);
        let (mut control_sender, incoming_control) = mpsc::channel(0);

        // Create identity_client:
        let rng = DummyRandom::new(&[1u8]);
        let pkcs8 = generate_pkcs8_key_pair(&rng);
        let identity = SoftwareEd25519Identity::from_pkcs8(&pkcs8).unwrap();
        let local_public_key = identity.get_public_key();
        let (requests_sender, identity_server) = create_identity(identity);
        spawner.spawn(identity_server.map(|_| ())).unwrap();
        let identity_client = IdentityClient::new(requests_sender);


        let server_conn = (client_sender, client_receiver);
        let rng = DummyRandom::new(&[2u8]);
        let first_server_time_hash = HashResult::from(&[1; HASH_RESULT_LEN]);

        let loop_fut = single_client_loop(server_conn,
                           incoming_control,
                           local_public_key.clone(),
                           identity_client,
                           rng,
                           first_server_time_hash)
            .map_err(|e| error!("single_client_loop() error: {:?}", e))
            .map(|_| ());

        spawner.spawn(loop_fut).unwrap();

        // Send some time hashes from server:
        for i in 2 .. 8 {
            let time_hash = HashResult::from(&[i; HASH_RESULT_LEN]);
            await!(server_sender.send(IndexServerToClient::TimeHash(time_hash))).unwrap();
        }

        // Request routes:
        let request_routes = RequestRoutes {
            request_id: Uid::from(&[3; UID_LEN]),
            capacity: 20,
            source: PublicKey::from(&[0xcc; PUBLIC_KEY_LEN]),
            destination: PublicKey::from(&[0xdd; PUBLIC_KEY_LEN]),
            opt_exclude: None,
        };

        let (response_sender, response_receiver) = oneshot::channel();
        let single_client_control = 
            SingleClientControl::RequestRoutes((request_routes.clone(), response_sender));

        await!(control_sender.send(single_client_control)).unwrap();

        // Request routes is redirected to server:
        let index_client_to_server = await!(server_receiver.next()).unwrap();
        match index_client_to_server {
            IndexClientToServer::RequestRoutes(sent_request_routes) => {
                assert_eq!(request_routes, sent_request_routes);
            },
            _ => unreachable!(),
        };

        // Server sends response routes:
        let response_routes = ResponseRoutes {
            request_id: Uid::from(&[3; UID_LEN]),
            routes: vec![], // No suitable routes were found
        };

        await!(server_sender.send(
                IndexServerToClient::ResponseRoutes(response_routes.clone()))).unwrap();

        // Client receives response routes:
        let routes = await!(response_receiver).unwrap();
        assert_eq!(routes, vec![]);


        for iter in 0 .. 3 { // Counter should increment every time
            // Send mutations:
            await!(control_sender.send(SingleClientControl::SendMutations(vec![]))).unwrap();

            // Mutations should be sent to the server:
            let index_client_to_server = await!(server_receiver.next()).unwrap();
            match index_client_to_server {
                IndexClientToServer::MutationsUpdate(mutations_update) => {
                    assert_eq!(mutations_update.node_public_key, local_public_key);
                    assert_eq!(mutations_update.index_mutations, vec![]);
                    // Counter should match iter:
                    assert_eq!(mutations_update.counter, iter); 
                    assert_eq!(mutations_update.time_hash, 
                               HashResult::from(&[7; HASH_RESULT_LEN]));

                    assert!(mutations_update.verify_signature());
                },
                _ => unreachable!(),
            };
        }
    }

    #[test]
    fn test_single_client_loop_basic() {
        let mut thread_pool = ThreadPool::new().unwrap();
        thread_pool.run(task_single_client_loop_basic(thread_pool.clone()));
    }
}


