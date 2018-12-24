use std::marker::Unpin;
use std::collections::HashMap;
use futures::{future, stream, Stream, StreamExt, Sink, SinkExt};
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
    Mutation,
    RequestRoutes};

type ServerConn = ConnPair<IndexClientToServer, IndexServerToClient>;

#[derive(Debug)]
pub enum SingleClientControl {
    RequestRoutes((RequestRoutes, oneshot::Sender<Vec<RouteWithCapacity>>)),
    SendMutations(Vec<Mutation>),
}

#[derive(Debug)]
pub enum SingleClientError {
    ControlClosed,
    ServerClosed,
    SendToServerError,
    RequestSignatureFailed,
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
            SingleClientControl::SendMutations(mutations) => {
                let mut mutations_update = MutationsUpdate {
                    node_public_key: self.local_public_key.clone(),
                    mutations,
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
                // We assume that the counter will never reach the wrapping point,
                // because it is of size at least 64 bits.
                self.counter = self.counter.wrapping_add(1);

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
#[allow(unused)]
async fn first_server_time_hash(from_server: &mut mpsc::Receiver<IndexServerToClient>)
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

#[allow(unused)]
async fn single_client_loop<IC,R>(server_conn: ServerConn,
                     incoming_control: IC,
                     local_public_key: PublicKey,
                     identity_client: IdentityClient,
                     rng: R,
                     session_id: Uid,
                     first_server_time_hash: HashResult) -> Result<(), SingleClientError>
where
    IC: Stream<Item=SingleClientControl> + Unpin,
    R: CryptoRandom,
{
    let (to_server, from_server) = server_conn;

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
