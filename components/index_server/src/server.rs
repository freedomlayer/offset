use std::marker::Unpin;
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};

use futures::{select, future, FutureExt, TryFutureExt, stream, 
    Stream, StreamExt, SinkExt};
use futures::channel::{mpsc, oneshot};
use futures::task::{Spawn, SpawnExt};

use common::conn::{ConnPair, FutTransform};

use crypto::identity::{PublicKey, compare_public_key};
use crypto::uid::Uid;

use timer::TimerClient;

use proto::index::messages::{IndexServerToClient, 
    IndexClientToServer, IndexServerToServer, 
    ResponseRoutes, RouteWithCapacity, MutationsUpdate,
    ForwardMutationsUpdate, Mutation, TimeProofLink};

use proto::funder::messages::FriendsRoute;

use crate::graph::graph_service::{GraphClient, GraphClientError};
use crate::verifier::Verifier;


type ServerConn = ConnPair<IndexServerToServer, IndexServerToServer>;
type ClientConn = ConnPair<IndexServerToClient, IndexClientToServer>;


#[derive(Debug)]
pub enum IndexServerError {
    SpawnError,
    RequestTimerStreamFailed,
    GraphClientError,
    ClientEventSenderError,
    ClientSenderError,
    RemoteSendError,
}

struct IndexServerConfig<A> {
    local_public_key: PublicKey,
    // map: public_key -> server_address
    trusted_servers: HashMap<PublicKey, A>, 
}

/// A connected remote entity
struct Connected<T> {
    opt_sender: Option<mpsc::Sender<T>>
}

impl<T> Connected<T> {
    pub fn new(sender: mpsc::Sender<T>) -> Self {
        Connected {
            opt_sender: Some(sender),
        }
    }

    /// Try to queue a message to remote entity. 
    /// Might fail because the queue is full.
    ///
    /// Ignores new send attempts after a real failure occurs.
    pub fn try_send(&mut self, t: T) 
        -> Result<(), IndexServerError> {

        if let Some(mut sender) = self.opt_sender.take() {
            if let Err(e) = sender.try_send(t) {
                if e.is_full() {
                    self.opt_sender = Some(sender);
                }
                return Err(IndexServerError::RemoteSendError)
            }
        }
        Ok(())
    }
}

struct ServerInitiating {
    #[allow(unused)]
    close_sender: oneshot::Sender<()>,
}

enum RemoteServerState {
    Connected(Connected<IndexServerToServer>),
    Initiating(ServerInitiating),
    Listening,
}

struct RemoteServer<A> {
    address: A,
    state: RemoteServerState,
}

struct IndexServer<A,S,SC,V> {
    local_public_key: PublicKey,
    server_connector: SC,
    graph_client: GraphClient<PublicKey,u128>,
    verifier: V,
    remote_servers: HashMap<PublicKey, RemoteServer<A>>,
    clients: HashMap<PublicKey, Connected<IndexServerToClient>>,
    event_sender: mpsc::Sender<IndexServerEvent>,
    spawner: S,
}

impl From<GraphClientError> for IndexServerError {
    fn from(from: GraphClientError) -> IndexServerError {
        IndexServerError::GraphClientError
    }
}

#[derive(Debug)]
enum IndexServerEvent {
    ServerConnection((PublicKey, ServerConn)),
    FromServer((PublicKey, Option<IndexServerToServer>)),
    ClientConnection((PublicKey, ClientConn)),
    ClientClosed(PublicKey),
    ClientMutationsUpdate(MutationsUpdate),
    TimerTick,
}

/// We divide servers into two types:
/// 1. "listen server": A trusted server which has the responsibility of connecting to us.
/// 2. "init server": A trusted server for which we have the responsibility to initiate
///    connection to.
///
/// Note: This function is not a method of IndexServer because we need to apply it as a filter
/// for the incoming server connections. Having it as a method of IndexServer will cause borrow
/// checker problems.
fn is_listen_server(local_public_key: &PublicKey, friend_public_key: &PublicKey) -> bool {
    compare_public_key(local_public_key, friend_public_key) == Ordering::Less
}


impl<A,S,SC,V> IndexServer<A,S,SC,V> 
where
    A: Clone + Send + std::fmt::Debug + 'static,
    S: Spawn + Send, 
    SC: FutTransform<Input=(PublicKey, A), Output=ServerConn> + Clone + Send + 'static,
    V: Verifier<Node=PublicKey, Neighbor=PublicKey, SessionId=Uid>,
{
    pub fn new(index_server_config: IndexServerConfig<A>,
               server_connector: SC,
               graph_client: GraphClient<PublicKey, u128>,
               verifier: V,
               event_sender: mpsc::Sender<IndexServerEvent>,
               spawner: S) -> Result<Self, IndexServerError> {

        let IndexServerConfig {local_public_key, trusted_servers} 
                = index_server_config;

        let mut index_server = IndexServer {
            local_public_key,
            server_connector,
            graph_client,
            verifier,
            remote_servers: HashMap::new(),
            clients: HashMap::new(),
            event_sender,
            spawner,
        };

        for (public_key, address) in trusted_servers.into_iter() {
            let remote_server = index_server.spawn_server(public_key.clone(), address)?;
            index_server.remote_servers.insert(public_key, remote_server);
        }
        Ok(index_server)
    }


    /// Iterate over all connected servers
    fn iter_connected_servers(&mut self) 
        -> impl Iterator<Item=(&PublicKey, &mut Connected<IndexServerToServer>)> {

        self.remote_servers
            .iter_mut()
            .filter_map(|(server_public_key, remote_server)| match &mut remote_server.state {
                RemoteServerState::Connected(server_connected) => Some((server_public_key, server_connected)),
                RemoteServerState::Initiating(_) | 
                RemoteServerState::Listening => None,
            })
    }

    pub fn spawn_server(&mut self, public_key: PublicKey, address: A) 
        -> Result<RemoteServer<A>, IndexServerError> {

        if is_listen_server(&self.local_public_key, &public_key) {
            return Ok(RemoteServer {
                address,
                state: RemoteServerState::Listening,
            })
        }

        println!("{:?}: Initiating connection to server... {:?}", self.local_public_key[0], public_key[0]);
        // We have the responsibility to initiate connection:
        let (close_sender, close_receiver) = oneshot::channel();
        let mut c_server_connector = self.server_connector.clone();
        let mut c_event_sender = self.event_sender.clone();
        let c_address = address.clone();

        let cancellable_fut = async move {
            let server_conn_fut = c_server_connector.transform((public_key.clone(), c_address));
            let select_res = select! {
                server_conn_fut = server_conn_fut.fuse() => Some(server_conn_fut),
                _close_receiver = close_receiver.fuse() => None,
            };
            if let Some(server_conn) = select_res {
                let _ = await!(c_event_sender.send(IndexServerEvent::ServerConnection((public_key, server_conn))));
            }
        };

        self.spawner.spawn(cancellable_fut)
            .map_err(|_| IndexServerError::SpawnError)?;

        let state = RemoteServerState::Initiating(ServerInitiating {
            close_sender,
        });

        Ok(RemoteServer {
            address,
            state,
        })
    }

    pub async fn handle_forward_mutations_update(&mut self, 
                                                 opt_server_public_key: Option<PublicKey>,
                                                 mut forward_mutations_update: ForwardMutationsUpdate)
                                                    -> Result<(), IndexServerError> 
    {
        // Check the signature:
        if !forward_mutations_update.mutations_update.verify_signature() {
            return Ok(())
        }

        // Make sure that the signature is fresh, and that the message is not out of order:
        // TODO: Maybe change signature to take a slice of slices intead of having to clone hashes
        // below? Requires change to Verifier trait interface.
        let expansion_chain = forward_mutations_update
            .time_proof_chain
            .iter()
            .map(|time_proof_link| &time_proof_link.hashes[..])
            .collect::<Vec<_>>();

        let mutations_update = &forward_mutations_update.mutations_update;
        let hashes = match self.verifier.verify(&mutations_update.time_hash, 
                             &expansion_chain,
                             &mutations_update.node_public_key,
                             &mutations_update.session_id,
                             mutations_update.counter) {
            Some(hashes) => hashes,
            None => return Ok(()),
        };

        // The message is valid and fresh.

        // Expire old edges for `node_public_key`:
        // Note: This tick happens every time a message is received from this `node_public_key`,
        // and not every constant amount of time. 
        self.graph_client.tick(mutations_update.node_public_key.clone());
        
        // Add a link to the time proof:
        forward_mutations_update
            .time_proof_chain
            .push(TimeProofLink {
                hashes: hashes.to_vec(),
            });

        // Apply mutations:
        for mutation in &mutations_update.mutations {
            match mutation {
                Mutation::UpdateFriend(update_friend) => {
                    await!(self.graph_client.update_edge(mutations_update.node_public_key.clone(), 
                                                  update_friend.public_key.clone(),
                                                  (update_friend.send_capacity,
                                                   update_friend.recv_capacity)))?;
                },
                Mutation::RemoveFriend(friend_public_key) => {
                    await!(self.graph_client.remove_edge(mutations_update.node_public_key.clone(), 
                                                         friend_public_key.clone()))?;
                },
            }
        }


        // Try to forward to all connected servers:
        for (server_public_key, connected_server) in self.iter_connected_servers() {
            if Some(server_public_key) == opt_server_public_key.as_ref() {
                // Don't send back to the server who sent this ForwardMutationsUpdate message
                continue;
            }
            let _ = connected_server.try_send(
                IndexServerToServer::ForwardMutationsUpdate(forward_mutations_update.clone()));
        }
        Ok(())
    }

    pub async fn handle_from_server(&mut self, public_key: PublicKey, server_msg: IndexServerToServer)
        -> Result<(), IndexServerError> {

        match server_msg {
            IndexServerToServer::TimeHash(time_hash) => {
                let _ = self.verifier.neighbor_tick(public_key, time_hash);
            },
            IndexServerToServer::ForwardMutationsUpdate(forward_mutations_update) => {
                await!(self.handle_forward_mutations_update(Some(public_key), forward_mutations_update))?;
            },
        };
        Ok(())
    }

    pub async fn handle_timer_tick(&mut self)
        -> Result<(), IndexServerError> {

        let (time_hash, removed_nodes) = self.verifier.tick();

        // Try to send the time tick to all servers. Sending to some of them might fail:
        for (_server_public_key, connected_server) in self.iter_connected_servers() {
            let _ = connected_server.try_send(IndexServerToServer::TimeHash(time_hash.clone()));
        }

        // Try to send time tick to all connected clients:
        for (_client_public_key, connected_client) in &mut self.clients {
            let _ = connected_client.try_send(IndexServerToClient::TimeHash(time_hash.clone()));
        }

        // Update the graph service about removed nodes:
        for node_public_key in removed_nodes {
            await!(self.graph_client.remove_node(node_public_key))?;
        }

        Ok(())
    }
}


async fn client_handler(mut graph_client: GraphClient<PublicKey, u128>,
                        public_key: PublicKey,
                        client_conn: ClientConn,
                        mut event_sender: mpsc::Sender<IndexServerEvent>) 
    -> Result<(), IndexServerError> {

    let (mut sender, mut receiver) = client_conn;

    while let Some(client_msg) = await!(receiver.next()) {
        match client_msg {
            IndexClientToServer::MutationsUpdate(mutations_update) => {
                // Forward to main server future to process:
                await!(event_sender.send(IndexServerEvent::ClientMutationsUpdate(mutations_update)))
                    .map_err(|_| IndexServerError::ClientEventSenderError)?;
            },
            IndexClientToServer::RequestRoutes(request_routes) => {
                let route_tuples = await!(graph_client.get_routes(request_routes.source.clone(), 
                                             request_routes.destination.clone(),
                                             request_routes.capacity,
                                             request_routes.opt_exclude.clone()))?;
                let routes = route_tuples
                    .into_iter()
                    .map(|(route, capacity)| RouteWithCapacity {
                        route: FriendsRoute { public_keys: route },
                        capacity,
                    })
                    .collect::<Vec<_>>();

                let response_routes = ResponseRoutes {
                    request_id: request_routes.request_id,
                    routes,
                };
                let message = IndexServerToClient::ResponseRoutes(response_routes);
                await!(sender.send(message))
                    .map_err(|_| IndexServerError::ClientSenderError)?;
            },
        }
    }
    Ok(())
}


async fn server_loop<A,IS,IC,SC,V,TS,S>(index_server_config: IndexServerConfig<A>,
                                 incoming_server_connections: IS,
                                 incoming_client_connections: IC,
                                 server_connector: SC,
                                 graph_client: GraphClient<PublicKey, u128>,
                                 verifier: V,
                                 mut timer_stream: TS,
                                 spawner: S) -> Result<(), IndexServerError>
where
    A: Clone + Send + std::fmt::Debug + 'static,
    IS: Stream<Item=(PublicKey, ServerConn)> + Unpin,
    IC: Stream<Item=(PublicKey, ClientConn)> + Unpin,
    SC: FutTransform<Input=(PublicKey, A), Output=ServerConn> + Clone + Send + 'static,
    V: Verifier<Node=PublicKey, Neighbor=PublicKey, SessionId=Uid>,
    TS: Stream + Unpin,
    S: Spawn + Send,
{

    // TODO: Create translation between incoming ticks (Which might happen pretty often)
    // to hash ticks, which should be a bit slower. (For every hash tick we have to send the hash
    // ticks to all servers). For example, every 16 incoming ticks will translate into one hash
    // tick.

    let (event_sender, event_receiver) = mpsc::channel(0);

    let mut index_server = IndexServer::new(
               index_server_config,
               server_connector,
               graph_client,
               verifier,
               event_sender,
               spawner)?;

    // We filter the incoming server connections, accepting connections only from servers
    // that are "listen servers". See docs of `is_listen_server`.
    let c_local_public_key = index_server.local_public_key.clone();
    let incoming_server_connections = incoming_server_connections
        .filter(|(server_public_key, _server_conn)| future::ready(is_listen_server(&c_local_public_key, &server_public_key)))
        .map(|server_connection| IndexServerEvent::ServerConnection(server_connection));

    let incoming_client_connections = incoming_client_connections
        .map(|client_connection| IndexServerEvent::ClientConnection(client_connection));

    let timer_stream = timer_stream
        .map(|_| IndexServerEvent::TimerTick);

    let mut events = event_receiver
        .select(incoming_server_connections)
        .select(incoming_client_connections)
        .select(timer_stream);

    while let Some(event) = await!(events.next()) {
        match event {
            IndexServerEvent::ServerConnection((public_key, server_conn)) => {
                let mut remote_server = match index_server.remote_servers.remove(&public_key) {
                    None => {
                        error!("Non trusted server {:?} attempted connection. Aborting.", public_key);
                        continue;
                    },
                    Some(remote_server) => remote_server,
                };


                match remote_server.state {
                    RemoteServerState::Connected(_) => {
                        error!("Server {:?} is already connected! Aborting.", public_key);
                        index_server.remote_servers.insert(public_key, remote_server);
                        continue;
                    },
                    RemoteServerState::Initiating(_) | 
                    RemoteServerState::Listening => {},
                };

                let (sender, receiver) = server_conn;

                remote_server.state = RemoteServerState::Connected(Connected::new(sender));

                let c_public_key = public_key.clone();
                let mut receiver = receiver
                    .map(move |msg| IndexServerEvent::FromServer((c_public_key.clone(), Some(msg))))
                    .chain(stream::once(future::ready(IndexServerEvent::FromServer((public_key.clone(), None)))));

                let mut c_event_sender = index_server.event_sender.clone();

                index_server.spawner.spawn(async move {
                    let _ = await!(c_event_sender.send_all(&mut receiver));
                });

                index_server.remote_servers.insert(public_key, remote_server);
            },
            IndexServerEvent::FromServer((public_key, Some(index_server_to_server))) => 
                await!(index_server.handle_from_server(public_key, index_server_to_server))?,
            IndexServerEvent::FromServer((public_key, None)) => {
                println!("{}: Connection closed {}", index_server.local_public_key[0], public_key[0]);
                // Server connection closed
                let old_server = match index_server.remote_servers.remove(&public_key) {
                    None => {
                        error!("A non existent server {:?} was closed. Aborting.", public_key);
                        continue;
                    },
                    Some(old_server) => old_server,
                };
                let _ = index_server.verifier.remove_neighbor(&public_key);
                let server = index_server.spawn_server(public_key.clone(), old_server.address)?;
                index_server.remote_servers.insert(public_key, server);

            },
            IndexServerEvent::ClientConnection((public_key, client_conn)) => {
                if index_server.clients.contains_key(&public_key) {
                    error!("Client {:?} already connected! Aborting.", public_key);
                    continue;
                }

                // Duplicate the client sender:
                let (ref sender, _) = client_conn;
                let c_sender = sender.clone();

                let mut c_event_sender = index_server.event_sender.clone();
                let c_public_key = public_key.clone();
                let client_handler_fut = client_handler(index_server.graph_client.clone(),
                                                        public_key.clone(),
                                                        client_conn,
                                                        index_server.event_sender.clone())
                    .map_err(|e| error!("client_handler() error: {:?}", e))
                    .then(|_| async move {
                        let _ = await!(c_event_sender.send(IndexServerEvent::ClientClosed(c_public_key)));
                    });

                index_server.spawner.spawn(client_handler_fut);
                index_server.clients.insert(public_key, Connected::new(c_sender));
            },
            IndexServerEvent::ClientMutationsUpdate(mutations_update) => {
                let forward_mutations_update = ForwardMutationsUpdate {
                    mutations_update,
                    time_proof_chain: Vec::new(),
                };
                await!(index_server.handle_forward_mutations_update(None, forward_mutations_update))?;
            },
            IndexServerEvent::ClientClosed(public_key) => {
                // Client connection closed
                if index_server.clients.remove(&public_key).is_none() {
                    error!("A non existent client {:?} was closed.", public_key);
                }
            },
            IndexServerEvent::TimerTick => await!(index_server.handle_timer_tick())?,
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::executor::ThreadPool;
    use futures::task::Spawn;

    use crypto::test_utils::DummyRandom;
    use crypto::identity::{SoftwareEd25519Identity,
                            generate_pkcs8_key_pair, PUBLIC_KEY_LEN,
                            PublicKey, Signature, SIGNATURE_LEN};
    use crypto::uid::UID_LEN;
    use crypto::crypto_rand::{RandValue, RAND_VALUE_LEN};

    use identity::{create_identity, IdentityClient};
    use common::dummy_connector::{DummyConnector, ConnRequest};
    use proto::index::messages::{RequestRoutes};

    use crate::graph::graph_service::GraphRequest;
    use crate::verifier::simple_verifier::SimpleVerifier;



    fn create_identity_client<S>(mut spawner: S, seed: &[u8]) -> IdentityClient
    where
        S: Spawn,
    {
        let rng = DummyRandom::new(&[1u8]);
        let pkcs8 = generate_pkcs8_key_pair(&rng);
        let identity = SoftwareEd25519Identity::from_pkcs8(&pkcs8).unwrap();
        let (requests_sender, identity_server) = create_identity(identity);
        let identity_client = IdentityClient::new(requests_sender);
        spawner.spawn(identity_server.then(|_| future::ready(()))).unwrap();
        identity_client
    }

    async fn task_index_server_loop_single_server<S>(mut spawner: S) 
    where
        S: Spawn + Clone + Send + 'static,
    {
        let server_pk = PublicKey::from(&[0; PUBLIC_KEY_LEN]);

        let index_server_config: IndexServerConfig<u32> = IndexServerConfig {
            local_public_key: server_pk.clone(),
            trusted_servers: HashMap::new(),
        };

        let (server_connections_sender, incoming_server_connections) = mpsc::channel(0);
        let (mut client_connections_sender, incoming_client_connections) = mpsc::channel(0);

        let (conn_request_sender, conn_request_receiver) = mpsc::channel(0);
        let server_connector = DummyConnector::new(conn_request_sender);

        let (mut tick_sender, timer_stream) = mpsc::channel::<()>(0);

        let (graph_requests_sender, mut graph_requests_receiver) = mpsc::channel(0);
        let graph_client = GraphClient::new(graph_requests_sender);

        // TODO: Do we have a way to create a mock SimpleVerifier that will work correctly?
        // Currently DummyVerifier will cause infinite cycles when forwarding messages.
        let rng = DummyRandom::new(&[0u8]);
        let verifier = SimpleVerifier::new(8, rng);

        let server_loop_fut = server_loop(index_server_config,
                    incoming_server_connections,
                    incoming_client_connections,
                    server_connector,
                    graph_client,
                    verifier,
                    timer_stream,
                    spawner.clone())
            .map_err(|e| error!("Error in server_loop(): {:?}", e))
            .map(|_| ());

        spawner.spawn(server_loop_fut).unwrap();

        let identity_client = create_identity_client(spawner.clone(), &[1,1]);
        let client_public_key = await!(identity_client.request_public_key()).unwrap();

        let (mut client_sender, server_receiver) = mpsc::channel(0);
        let (server_sender, mut client_receiver) = mpsc::channel(0);
        await!(client_connections_sender.send((client_public_key.clone(), (server_sender, server_receiver)))).unwrap();


        // Client requests routes:
        let request_id = Uid::from(&[0; UID_LEN]);
        let request_routes = RequestRoutes {
            request_id: request_id.clone(),
            capacity: 100,
            source: PublicKey::from(&[8; PUBLIC_KEY_LEN]),
            destination: PublicKey::from(&[9; PUBLIC_KEY_LEN]),
            opt_exclude: None,
        };
        await!(client_sender.send(IndexClientToServer::RequestRoutes(request_routes))).unwrap();

        // Handle the graph request:
        match await!(graph_requests_receiver.next()).unwrap() {
            GraphRequest::GetRoutes(src, dest, capacity, opt_exclude, response_sender) => {
                assert_eq!(src, PublicKey::from(&[8; PUBLIC_KEY_LEN]));
                assert_eq!(dest, PublicKey::from(&[9; PUBLIC_KEY_LEN]));
                assert_eq!(capacity, 100);
                assert_eq!(opt_exclude, None);
                response_sender.send(Vec::new()).unwrap();
            }
            _ => unreachable!(),
        }

        match await!(client_receiver.next()).unwrap() {
            IndexServerToClient::ResponseRoutes(response_routes) => {
                assert_eq!(response_routes.request_id, request_id);
                assert!(response_routes.routes.is_empty());
            },
            _ => unreachable!(),
        };


        // Server should periodically send time hashes to the client:
        await!(tick_sender.send(())).unwrap();

        let time_hash = match await!(client_receiver.next()).unwrap() {
            IndexServerToClient::TimeHash(time_hash) => time_hash,
            _ => unreachable!(),
        };

        // Send mutations update to the server:
        let mutations = vec![Mutation::RemoveFriend(PublicKey::from(&[11; PUBLIC_KEY_LEN]))];

        let mut mutations_update = MutationsUpdate {
            node_public_key: client_public_key.clone(),
            mutations,
            time_hash,
            session_id: Uid::from(&[0; UID_LEN]),
            counter: 0,
            rand_nonce: RandValue::from(&[0; RAND_VALUE_LEN]),
            signature: Signature::from(&[0; SIGNATURE_LEN]),
        };

        // Calculate signature:
        mutations_update.signature = await!(identity_client.request_signature(
            mutations_update.signature_buff().clone())).unwrap();

        await!(client_sender.send(IndexClientToServer::MutationsUpdate(mutations_update))).unwrap();

        // Handle the graph request:
        match await!(graph_requests_receiver.next()).unwrap() {
            GraphRequest::RemoveEdge(src, dest, response_sender) => {
                assert_eq!(src, client_public_key);
                assert_eq!(dest, PublicKey::from(&[11; PUBLIC_KEY_LEN]));
                response_sender.send(None).unwrap();
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn test_index_server_loop_single_server() {
        let mut thread_pool = ThreadPool::new().unwrap();
        thread_pool.run(task_index_server_loop_single_server(thread_pool.clone()));
    }

    // ###########################################################
    // ###########################################################

    struct TestServer {
        public_key: PublicKey,
        server_connections_sender: mpsc::Sender<(PublicKey, ServerConn)>,
        client_connections_sender: mpsc::Sender<(PublicKey, ClientConn)>,
        graph_requests_receiver: mpsc::Receiver<GraphRequest<PublicKey, u128>>,
        server_conn_request_receiver: mpsc::Receiver<ConnRequest<(PublicKey, u8), ServerConn>>,
    }

    fn create_test_server<S>(server_public_key: PublicKey, 
                          trusted_servers: &[PublicKey], 
                          mut spawner: S) 
        -> TestServer

    where
        S: Spawn + Clone + Send + 'static,
    {
        let trusted_servers = trusted_servers
            .iter()
            .enumerate()
            .map(|(i, public_key)| (public_key.clone(), i as u8))
            .collect::<HashMap<_,_>>();

        let index_server_config: IndexServerConfig<u8> = IndexServerConfig {
            local_public_key: server_public_key.clone(),
            trusted_servers,
        };

        let (server_connections_sender, incoming_server_connections) = mpsc::channel(0);
        let (mut client_connections_sender, incoming_client_connections) = mpsc::channel(0);

        let (server_conn_request_sender, server_conn_request_receiver) = mpsc::channel(0);
        let server_connector = DummyConnector::new(server_conn_request_sender);

        let (mut tick_sender, timer_stream) = mpsc::channel::<()>(0);

        let (graph_requests_sender, mut graph_requests_receiver) = mpsc::channel(0);
        let graph_client = GraphClient::new(graph_requests_sender);

        let rng = DummyRandom::new(&[0x13, 0x37, server_public_key[0]]);
        let verifier = SimpleVerifier::new(8, rng);

        let server_loop_fut = server_loop(index_server_config,
                    incoming_server_connections,
                    incoming_client_connections,
                    server_connector,
                    graph_client,
                    verifier,
                    timer_stream,
                    spawner.clone())
            .map_err(|e| error!("Error in server_loop(): {:?}", e))
            .map(|_| ());

        spawner.spawn(server_loop_fut).unwrap();

        TestServer {
            public_key: server_public_key.clone(),
            server_connections_sender,
            client_connections_sender,
            graph_requests_receiver,
            server_conn_request_receiver,
        }

    }

    async fn handle_connect(test_servers: &mut [TestServer], from_index: usize) {
        let conn_request = await!(test_servers[from_index].server_conn_request_receiver.next()).unwrap();
        let (a_sender, b_receiver) = mpsc::channel(0);
        let (b_sender, a_receiver) = mpsc::channel(0);

        let (_dest_public_key, dest_index) = conn_request.address.clone();
        await!(test_servers[dest_index as usize].server_connections_sender.send(
                (test_servers[from_index].public_key.clone(), (b_sender, b_receiver)))).unwrap();

        conn_request.reply((a_sender, a_receiver));
    }

    async fn task_index_server_loop_multi_server<S>(mut spawner: S) 
    where
        S: Spawn + Clone + Send + 'static,
    {
        /*
         *  Servers layout: 
         *
         *    0 -- 1
         *    |    |
         *    2 -- 3 -- 4
        */

        // Server public keys:
        let mut spks = (0 .. 5)
            .map(|i| PublicKey::from(&[i; PUBLIC_KEY_LEN]))
            .collect::<Vec<_>>();

        // Sort public keys. This helps with testing, because we know how ties will be broken to
        // decide which server listens and which server initiates connection.
        spks.sort_by(compare_public_key);

        for (i, spk) in spks.iter().enumerate() {
            println!("index: {}, spk: {}", i, spk[0]);
        }

        let mut test_servers = Vec::new();
        test_servers.push(create_test_server(spks[0].clone(), &[spks[1].clone(),spks[2].clone()], spawner.clone()));
        test_servers.push(create_test_server(spks[1].clone(), &[spks[0].clone(),spks[3].clone()], spawner.clone()));
        test_servers.push(create_test_server(spks[2].clone(), &[spks[0].clone(),spks[3].clone()], spawner.clone()));
        test_servers.push(create_test_server(spks[3].clone(), &[spks[1].clone(),spks[2].clone(),spks[4].clone()], spawner.clone()));
        test_servers.push(create_test_server(spks[4].clone(), &[spks[3].clone()], spawner.clone()));

        await!(handle_connect(&mut test_servers[..], 4)); // 4 connects to {3}
        await!(handle_connect(&mut test_servers[..], 3)); // 3 connects to {1,2}
        await!(handle_connect(&mut test_servers[..], 3));
        await!(handle_connect(&mut test_servers[..], 3));
        await!(handle_connect(&mut test_servers[..], 2)); // 2 connects to {0}
        await!(handle_connect(&mut test_servers[..], 1)); // 1 connects to {0}
    }

    #[test]
    fn test_index_server_loop_multi_server() {
        let mut thread_pool = ThreadPool::new().unwrap();
        thread_pool.run(task_index_server_loop_multi_server(thread_pool.clone()));
    }
}

