use std::cmp::Ordering;
use std::collections::HashMap;
use std::marker::Unpin;

use futures::channel::{mpsc, oneshot};
use futures::task::{Spawn, SpawnExt};
use futures::{future, select, stream, FutureExt, SinkExt, Stream, StreamExt, TryFutureExt};

use common::conn::{ConnPair, FutTransform};
use common::select_streams::{select_streams, BoxStream};

use crypto::identity::PublicKey;
use crypto::uid::Uid;

use proto::index_server::messages::{
    ForwardMutationsUpdate, IndexClientToServer, IndexMutation, IndexServerToClient,
    IndexServerToServer, MutationsUpdate, ResponseRoutes, MultiRoute, TimeProofLink,
};

use proto::funder::messages::FriendsRoute;

use crate::graph::graph_service::{GraphClient, GraphClientError};
use crate::verifier::Verifier;

pub type ServerConn = ConnPair<IndexServerToServer, IndexServerToServer>;
pub type ClientConn = ConnPair<IndexServerToClient, IndexClientToServer>;

#[derive(Debug)]
pub enum ServerLoopError {
    SpawnError,
    RequestTimerStreamFailed,
    GraphClientError,
    ClientEventSenderError,
    ClientSenderError,
    RemoteSendError,
}

/// A connected remote entity
#[derive(Debug)]
struct Connected<T> {
    opt_sender: Option<mpsc::Sender<T>>,
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
    pub fn try_send(&mut self, t: T) -> Result<(), ServerLoopError> {
        if let Some(mut sender) = self.opt_sender.take() {
            if let Err(e) = sender.try_send(t) {
                if e.is_full() {
                    warn!("try_send() failed: {:?}", e);
                    self.opt_sender = Some(sender);
                }
                return Err(ServerLoopError::RemoteSendError);
            }
            self.opt_sender = Some(sender);
        }
        Ok(())
    }
}

#[derive(Debug)]
struct ServerInitiating {
    #[allow(unused)]
    close_sender: oneshot::Sender<()>,
}

#[derive(Debug)]
enum RemoteServerState {
    Connected(Connected<IndexServerToServer>),
    Initiating(ServerInitiating),
    Listening,
}

#[derive(Debug)]
struct RemoteServer<A> {
    address: A,
    state: RemoteServerState,
}

struct IndexServer<A, S, SC, V, CMP> {
    local_public_key: PublicKey,
    server_connector: SC,
    graph_client: GraphClient<PublicKey, u128>,
    verifier: V,
    compare_public_key: CMP,
    remote_servers: HashMap<PublicKey, RemoteServer<A>>,
    clients: HashMap<PublicKey, Connected<IndexServerToClient>>,
    event_sender: mpsc::Sender<IndexServerEvent>,
    spawner: S,
}

impl From<GraphClientError> for ServerLoopError {
    fn from(_from: GraphClientError) -> ServerLoopError {
        ServerLoopError::GraphClientError
    }
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
enum IndexServerEvent {
    ServerConnection((PublicKey, ServerConn)),
    FromServer((PublicKey, Option<IndexServerToServer>)),
    ClientConnection((PublicKey, ClientConn)),
    ClientClosed(PublicKey),
    ClientMutationsUpdate(MutationsUpdate),
    TimerTick,
    ClientListenerClosed,
    ServerListenerClosed,
}

/*
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
*/

impl<A, S, SC, V, CMP> IndexServer<A, S, SC, V, CMP>
where
    A: Clone + Send + std::fmt::Debug + 'static,
    S: Spawn + Send,
    SC: FutTransform<Input = (PublicKey, A), Output = Option<ServerConn>> + Clone + Send + 'static,
    V: Verifier<Node = PublicKey, Neighbor = PublicKey, SessionId = Uid>,
    CMP: Clone + Fn(&PublicKey, &PublicKey) -> Ordering,
{
    pub fn new(
        local_public_key: PublicKey,
        trusted_servers: HashMap<PublicKey, A>,
        server_connector: SC,
        graph_client: GraphClient<PublicKey, u128>,
        compare_public_key: CMP,
        verifier: V,
        event_sender: mpsc::Sender<IndexServerEvent>,
        spawner: S,
    ) -> Result<Self, ServerLoopError> {
        let mut index_server = IndexServer {
            local_public_key,
            server_connector,
            graph_client,
            verifier,
            compare_public_key,
            remote_servers: HashMap::new(),
            clients: HashMap::new(),
            event_sender,
            spawner,
        };

        for (public_key, address) in trusted_servers.into_iter() {
            let remote_server = index_server.spawn_server(public_key.clone(), address)?;
            index_server
                .remote_servers
                .insert(public_key, remote_server);
        }
        Ok(index_server)
    }

    /// Iterate over all connected servers
    fn iter_connected_servers(
        &mut self,
    ) -> impl Iterator<Item = (&PublicKey, &mut Connected<IndexServerToServer>)> {
        self.remote_servers
            .iter_mut()
            .filter_map(
                |(server_public_key, remote_server)| match &mut remote_server.state {
                    RemoteServerState::Connected(server_connected) => {
                        Some((server_public_key, server_connected))
                    }
                    RemoteServerState::Initiating(_) | RemoteServerState::Listening => None,
                },
            )
    }

    pub fn spawn_server(
        &mut self,
        public_key: PublicKey,
        address: A,
    ) -> Result<RemoteServer<A>, ServerLoopError> {
        if (self.compare_public_key)(&self.local_public_key, &public_key) == Ordering::Less {
            return Ok(RemoteServer {
                address,
                state: RemoteServerState::Listening,
            });
        }

        // We have the responsibility to initiate connection:
        let (close_sender, close_receiver) = oneshot::channel();
        let mut c_server_connector = self.server_connector.clone();
        let mut c_event_sender = self.event_sender.clone();
        let c_address = address.clone();

        let cancellable_fut = async move {
            let server_conn_fut = c_server_connector.transform((public_key.clone(), c_address));
            let select_res = select! {
                server_conn_fut = server_conn_fut.fuse() => server_conn_fut,
                _close_receiver = close_receiver.fuse() => None,
            };
            if let Some(server_conn) = select_res {
                let _ = await!(c_event_sender.send(IndexServerEvent::ServerConnection((
                    public_key,
                    server_conn
                ))));
            } else {
                // Failed to connect, report that the server was closed
                // TODO: Test this functionality:
                let _ =
                    await!(c_event_sender.send(IndexServerEvent::FromServer((public_key, None))));
            }
        };

        self.spawner
            .spawn(cancellable_fut)
            .map_err(|_| ServerLoopError::SpawnError)?;

        let state = RemoteServerState::Initiating(ServerInitiating { close_sender });

        Ok(RemoteServer { address, state })
    }

    pub async fn handle_forward_mutations_update(
        &mut self,
        opt_server_public_key: Option<PublicKey>,
        mut forward_mutations_update: ForwardMutationsUpdate,
    ) -> Result<(), ServerLoopError> {
        // Check the signature:
        if !forward_mutations_update.mutations_update.verify_signature() {
            warn!(
                "{}: handle_forward_mutations_update: Failed verifying signature from server {:?}",
                self.local_public_key[0], opt_server_public_key
            );
            return Ok(());
        }

        // Make sure that the signature is fresh, and that the message is not out of order:
        // TODO: Maybe change signature to take a slice of slices instead of having to clone hashes
        // below? Requires change to Verifier trait interface.
        let expansion_chain = forward_mutations_update
            .time_proof_chain
            .iter()
            .map(|time_proof_link| &time_proof_link.hashes[..])
            .collect::<Vec<_>>();

        let mutations_update = &forward_mutations_update.mutations_update;
        let hashes = match self.verifier.verify(
            &mutations_update.time_hash,
            &expansion_chain,
            &mutations_update.node_public_key,
            &mutations_update.session_id,
            mutations_update.counter,
        ) {
            Some(hashes) => hashes,
            None => {
                warn!("{}: handle_forward_mutations_update: Failed verifying message from server {:?}",
                      self.local_public_key[0],
                      opt_server_public_key);
                return Ok(());
            }
        };

        // The message is valid and fresh.

        // Expire old edges for `node_public_key`:
        // Note: This tick happens every time a message is received from this `node_public_key`,
        // and not every constant amount of time.
        await!(self
            .graph_client
            .tick(mutations_update.node_public_key.clone()))
        .map_err(|_| ServerLoopError::GraphClientError)?;

        // Add a link to the time proof:
        forward_mutations_update
            .time_proof_chain
            .push(TimeProofLink {
                hashes: hashes.to_vec(),
            });

        // Apply mutations:
        for index_mutation in &mutations_update.index_mutations {
            match index_mutation {
                IndexMutation::UpdateFriend(update_friend) => {
                    info!(
                        "pk: {}, send: {}, recv: {}, rate: {:?}",
                        update_friend.public_key[0],
                        update_friend.send_capacity,
                        update_friend.recv_capacity,
                        update_friend.rate,
                    );

                    await!(self.graph_client.update_edge(
                        mutations_update.node_public_key.clone(),
                        update_friend.public_key.clone(),
                        (update_friend.send_capacity, update_friend.recv_capacity)
                    ))?;
                }
                IndexMutation::RemoveFriend(friend_public_key) => {
                    await!(self.graph_client.remove_edge(
                        mutations_update.node_public_key.clone(),
                        friend_public_key.clone()
                    ))?;
                }
            }
        }

        // Try to forward to all connected servers:
        for (server_public_key, connected_server) in self.iter_connected_servers() {
            if Some(server_public_key) == opt_server_public_key.as_ref() {
                // Don't send back to the server who sent this ForwardMutationsUpdate message
                continue;
            }
            let _ = connected_server.try_send(IndexServerToServer::ForwardMutationsUpdate(
                forward_mutations_update.clone(),
            ));
        }
        Ok(())
    }

    pub async fn handle_from_server(
        &mut self,
        public_key: PublicKey,
        server_msg: IndexServerToServer,
    ) -> Result<(), ServerLoopError> {
        match server_msg {
            IndexServerToServer::TimeHash(time_hash) => {
                let _ = self.verifier.neighbor_tick(public_key, time_hash);
            }
            IndexServerToServer::ForwardMutationsUpdate(forward_mutations_update) => {
                await!(self
                    .handle_forward_mutations_update(Some(public_key), forward_mutations_update))?;
            }
        };
        Ok(())
    }

    pub async fn handle_timer_tick(&mut self) -> Result<(), ServerLoopError> {
        let (time_hash, removed_nodes) = self.verifier.tick();

        // Try to send the time tick to all servers. Sending to some of them might fail:
        for (_server_public_key, connected_server) in self.iter_connected_servers() {
            let _ = connected_server.try_send(IndexServerToServer::TimeHash(time_hash.clone()));
        }

        // Try to send time tick to all connected clients:
        for connected_client in self.clients.values_mut() {
            let _ = connected_client.try_send(IndexServerToClient::TimeHash(time_hash.clone()));
        }

        // Update the graph service about removed nodes:
        for node_public_key in removed_nodes {
            await!(self.graph_client.remove_node(node_public_key))?;
        }

        Ok(())
    }
}

async fn client_handler(
    mut graph_client: GraphClient<PublicKey, u128>,
    _public_key: PublicKey, // TODO: unused?
    client_conn: ClientConn,
    mut event_sender: mpsc::Sender<IndexServerEvent>,
) -> Result<(), ServerLoopError> {
    let (mut sender, mut receiver) = client_conn;

    while let Some(client_msg) = await!(receiver.next()) {
        match client_msg {
            IndexClientToServer::MutationsUpdate(mutations_update) => {
                // Forward to main server future to process:
                await!(event_sender.send(IndexServerEvent::ClientMutationsUpdate(mutations_update)))
                    .map_err(|_| ServerLoopError::ClientEventSenderError)?;
            }
            IndexClientToServer::RequestRoutes(request_routes) => {
                let graph_multi_routes = await!(graph_client.get_routes(
                    request_routes.source.clone(),
                    request_routes.destination.clone(),
                    request_routes.capacity,
                    request_routes.opt_exclude.clone()
                ))?;
                let routes = graph_multi_routes
                    .into_iter()
                    .map(|graph_multi_route| MultiRoute {
                        routes: graph_multi_route.routes.map(|graph_route| RouteCapacityRate {
                            route: FriendRoute {public_keys: graph_route.route},
                            capacity: graph_route.capacity,
                            rate: graph_route.rate,
                        }),
                    })
                    .collect::<Vec<_>>();

                let response_routes = ResponseRoutes {
                    request_id: request_routes.request_id,
                    routes,
                };
                let message = IndexServerToClient::ResponseRoutes(response_routes);
                await!(sender.send(message)).map_err(|_| ServerLoopError::ClientSenderError)?;
            }
        }
    }
    Ok(())
}

pub async fn server_loop<A, IS, IC, SC, CMP, V, TS, S>(
    local_public_key: PublicKey,
    trusted_servers: HashMap<PublicKey, A>,
    incoming_server_connections: IS,
    incoming_client_connections: IC,
    server_connector: SC,
    graph_client: GraphClient<PublicKey, u128>,
    compare_public_key: CMP,
    verifier: V,
    timer_stream: TS,
    spawner: S,
    mut opt_debug_event_sender: Option<mpsc::Sender<()>>,
) -> Result<(), ServerLoopError>
where
    A: Clone + Send + std::fmt::Debug + 'static,
    IS: Stream<Item = (PublicKey, ServerConn)> + Unpin + Send,
    IC: Stream<Item = (PublicKey, ClientConn)> + Unpin + Send,
    SC: FutTransform<Input = (PublicKey, A), Output = Option<ServerConn>> + Clone + Send + 'static,
    V: Verifier<Node = PublicKey, Neighbor = PublicKey, SessionId = Uid>,
    CMP: Clone + Fn(&PublicKey, &PublicKey) -> Ordering + Sync,
    TS: Stream + Unpin + Send,
    S: Spawn + Send,
{
    // TODO: Create translation between incoming ticks (Which might happen pretty often)
    // to hash ticks, which should be a bit slower. (For every hash tick we have to send the hash
    // ticks to all servers). For example, every 16 incoming ticks will translate into one hash
    // tick.

    let (event_sender, event_receiver) = mpsc::channel(0);

    let mut index_server = IndexServer::new(
        local_public_key,
        trusted_servers,
        server_connector,
        graph_client,
        compare_public_key,
        verifier,
        event_sender,
        spawner,
    )?;

    // We filter the incoming server connections, accepting connections only from servers
    // that are "listen servers".
    let c_local_public_key = index_server.local_public_key.clone();
    let c_compare_public_key = index_server.compare_public_key.clone();
    let incoming_server_connections = incoming_server_connections
        .filter(|(server_public_key, _server_conn)| {
            future::ready(
                c_compare_public_key(&c_local_public_key, &server_public_key) == Ordering::Less,
            )
        })
        .map(IndexServerEvent::ServerConnection)
        .chain(stream::once(future::ready(
            IndexServerEvent::ServerListenerClosed,
        )));

    let incoming_client_connections = incoming_client_connections
        .map(IndexServerEvent::ClientConnection)
        .chain(stream::once(future::ready(
            IndexServerEvent::ClientListenerClosed,
        )));

    let timer_stream = timer_stream.map(|_| IndexServerEvent::TimerTick);

    let mut events = select_streams![
        event_receiver,
        incoming_server_connections,
        incoming_client_connections,
        timer_stream
    ];

    while let Some(event) = await!(events.next()) {
        match event {
            IndexServerEvent::ServerConnection((public_key, server_conn)) => {
                let mut remote_server = match index_server.remote_servers.remove(&public_key) {
                    None => {
                        error!(
                            "Non trusted server {:?} attempted connection. Aborting.",
                            public_key
                        );
                        continue;
                    }
                    Some(remote_server) => remote_server,
                };

                match remote_server.state {
                    RemoteServerState::Connected(_) => {
                        error!("Server {:?} is already connected! Aborting.", public_key);
                        index_server
                            .remote_servers
                            .insert(public_key, remote_server);
                        continue;
                    }
                    RemoteServerState::Initiating(_) | RemoteServerState::Listening => {}
                };

                let (sender, receiver) = server_conn;

                remote_server.state = RemoteServerState::Connected(Connected::new(sender));

                let c_public_key = public_key.clone();
                let mut receiver = receiver
                    .map(move |msg| IndexServerEvent::FromServer((c_public_key.clone(), Some(msg))))
                    .chain(stream::once(future::ready(IndexServerEvent::FromServer((
                        public_key.clone(),
                        None,
                    )))));

                let mut c_event_sender = index_server.event_sender.clone();

                index_server
                    .spawner
                    .spawn(async move {
                        let _ = await!(c_event_sender.send_all(&mut receiver));
                    })
                    .map_err(|_| ServerLoopError::SpawnError)?;

                index_server
                    .remote_servers
                    .insert(public_key, remote_server);
            }
            IndexServerEvent::FromServer((public_key, Some(index_server_to_server))) => {
                await!(index_server.handle_from_server(public_key, index_server_to_server))?
            }
            IndexServerEvent::FromServer((public_key, None)) => {
                // Server connection closed
                let old_server = match index_server.remote_servers.remove(&public_key) {
                    None => {
                        error!(
                            "A non existent server {:?} was closed. Aborting.",
                            public_key
                        );
                        continue;
                    }
                    Some(old_server) => old_server,
                };
                let _ = index_server.verifier.remove_neighbor(&public_key);
                let server = index_server.spawn_server(public_key.clone(), old_server.address)?;
                index_server.remote_servers.insert(public_key, server);
            }
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
                let client_handler_fut = client_handler(
                    index_server.graph_client.clone(),
                    public_key.clone(),
                    client_conn,
                    index_server.event_sender.clone(),
                )
                .map_err(|e| error!("client_handler() error: {:?}", e))
                .then(|_| {
                    async move {
                        let _ = await!(
                            c_event_sender.send(IndexServerEvent::ClientClosed(c_public_key))
                        );
                    }
                });

                index_server
                    .spawner
                    .spawn(client_handler_fut)
                    .map_err(|_| ServerLoopError::SpawnError)?;
                index_server
                    .clients
                    .insert(public_key, Connected::new(c_sender));
            }
            IndexServerEvent::ClientMutationsUpdate(mutations_update) => {
                let forward_mutations_update = ForwardMutationsUpdate {
                    mutations_update,
                    time_proof_chain: Vec::new(),
                };
                await!(
                    index_server.handle_forward_mutations_update(None, forward_mutations_update)
                )?;
            }
            IndexServerEvent::ClientClosed(public_key) => {
                // Client connection closed
                if index_server.clients.remove(&public_key).is_none() {
                    error!("A non existent client {:?} was closed.", public_key);
                }
            }
            IndexServerEvent::TimerTick => await!(index_server.handle_timer_tick())?,
            IndexServerEvent::ClientListenerClosed => {
                warn!("server_loop() client listener closed!");
                break;
            }
            IndexServerEvent::ServerListenerClosed => {
                warn!("server_loop() server listener closed!");
                break;
            }
        }
        // debug_event_sender is used to control the order of events during testing.
        // This is useful to have deterministic test results.
        // Normal code should have no use of this sender, and it should be set to None.
        if let Some(ref mut debug_event_sender) = &mut opt_debug_event_sender {
            let _ = await!(debug_event_sender.send(()));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::executor::ThreadPool;
    use futures::task::Spawn;

    use crypto::crypto_rand::{RandValue, RAND_VALUE_LEN};
    use crypto::identity::{
        generate_pkcs8_key_pair, PublicKey, Signature, SoftwareEd25519Identity, PUBLIC_KEY_LEN,
        SIGNATURE_LEN,
    };
    use crypto::test_utils::DummyRandom;
    use crypto::uid::UID_LEN;

    use common::dummy_connector::{ConnRequest, DummyConnector};
    use identity::{create_identity, IdentityClient};
    use proto::index_server::messages::RequestRoutes;

    use crate::graph::graph_service::GraphRequest;
    use crate::verifier::simple_verifier::SimpleVerifier;

    /// Size of channel used for channels between servers, or channels between a server and a
    /// client. A value bigger than 0 is used to make sure try_send() doesn't fail during mutations
    /// forwarding or when sending time hash ticks.
    const CHANNEL_SIZE: usize = 16;

    fn create_identity_client<S>(mut spawner: S, seed: &[u8]) -> IdentityClient
    where
        S: Spawn,
    {
        let rng = DummyRandom::new(seed);
        let pkcs8 = generate_pkcs8_key_pair(&rng);
        let identity = SoftwareEd25519Identity::from_pkcs8(&pkcs8).unwrap();
        let (requests_sender, identity_server) = create_identity(identity);
        let identity_client = IdentityClient::new(requests_sender);
        spawner
            .spawn(identity_server.then(|_| future::ready(())))
            .unwrap();
        identity_client
    }

    async fn task_index_server_loop_single_server<S>(mut spawner: S)
    where
        S: Spawn + Clone + Send + 'static,
    {
        let server_pk = PublicKey::from(&[0; PUBLIC_KEY_LEN]);

        let local_public_key = server_pk.clone();
        let trusted_servers: HashMap<PublicKey, u8> = HashMap::new();

        let (_server_connections_sender, incoming_server_connections) = mpsc::channel(0);
        let (mut client_connections_sender, incoming_client_connections) = mpsc::channel(0);

        let (conn_request_sender, _conn_request_receiver) = mpsc::channel(0);
        let server_connector = DummyConnector::new(conn_request_sender);

        let (mut tick_sender, timer_stream) = mpsc::channel::<()>(0);

        let (graph_requests_sender, mut graph_requests_receiver) = mpsc::channel(0);
        let graph_client = GraphClient::new(graph_requests_sender);

        let compare_public_key = |pk_a: &PublicKey, pk_b: &PublicKey| pk_a.cmp(pk_b);

        // TODO: Do we have a way to create a mock SimpleVerifier that will work correctly?
        // Currently DummyVerifier will cause infinite cycles when forwarding messages.
        let rng = DummyRandom::new(&[0u8]);
        let verifier = SimpleVerifier::new(8, rng);

        let server_loop_fut = server_loop(
            local_public_key,
            trusted_servers,
            incoming_server_connections,
            incoming_client_connections,
            server_connector,
            graph_client,
            compare_public_key,
            verifier,
            timer_stream,
            spawner.clone(),
            None,
        )
        .map_err(|e| error!("Error in server_loop(): {:?}", e))
        .map(|_| ());

        spawner.spawn(server_loop_fut).unwrap();

        let identity_client = create_identity_client(spawner.clone(), &[1, 1]);
        let client_public_key = await!(identity_client.request_public_key()).unwrap();

        let (mut client_sender, server_receiver) = mpsc::channel(CHANNEL_SIZE);
        let (server_sender, mut client_receiver) = mpsc::channel(CHANNEL_SIZE);
        await!(client_connections_sender
            .send((client_public_key.clone(), (server_sender, server_receiver))))
        .unwrap();

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
                assert!(response_routes.multi_routes.is_empty());
            }
            _ => unreachable!(),
        };

        // Server should periodically send time hashes to the client:
        await!(tick_sender.send(())).unwrap();

        let time_hash = match await!(client_receiver.next()).unwrap() {
            IndexServerToClient::TimeHash(time_hash) => time_hash,
            _ => unreachable!(),
        };

        // Send mutations update to the server:
        let index_mutations = vec![IndexMutation::RemoveFriend(PublicKey::from(
            &[11; PUBLIC_KEY_LEN],
        ))];

        let mut mutations_update = MutationsUpdate {
            node_public_key: client_public_key.clone(),
            index_mutations,
            time_hash,
            session_id: Uid::from(&[0; UID_LEN]),
            counter: 0,
            rand_nonce: RandValue::from(&[0; RAND_VALUE_LEN]),
            signature: Signature::from(&[0; SIGNATURE_LEN]),
        };

        // Calculate signature:
        mutations_update.signature =
            await!(identity_client.request_signature(mutations_update.signature_buff().clone()))
                .unwrap();

        await!(client_sender.send(IndexClientToServer::MutationsUpdate(mutations_update))).unwrap();

        // Handle tick request:
        match await!(graph_requests_receiver.next()).unwrap() {
            GraphRequest::Tick(node, response_sender) => {
                assert_eq!(node, client_public_key);
                response_sender.send(()).unwrap();
            }
            _ => unreachable!(),
        }

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
        tick_sender: mpsc::Sender<()>,
        server_connections_sender: mpsc::Sender<(PublicKey, ServerConn)>,
        client_connections_sender: mpsc::Sender<(PublicKey, ClientConn)>,
        graph_requests_receiver: mpsc::Receiver<GraphRequest<PublicKey, u128>>,
        server_conn_request_receiver:
            mpsc::Receiver<ConnRequest<(PublicKey, u8), Option<ServerConn>>>,
        debug_event_receiver: mpsc::Receiver<()>,
    }

    fn create_test_server<S>(index: u8, trusted_servers: &[u8], mut spawner: S) -> TestServer
    where
        S: Spawn + Clone + Send + 'static,
    {
        let server_public_key = PublicKey::from(&[index; PUBLIC_KEY_LEN]);
        let trusted_servers = trusted_servers
            .iter()
            .map(|&i| (PublicKey::from(&[i; PUBLIC_KEY_LEN]), i))
            .collect::<HashMap<_, _>>();

        let local_public_key = server_public_key.clone();

        let (server_connections_sender, incoming_server_connections) = mpsc::channel(0);
        let (client_connections_sender, incoming_client_connections) = mpsc::channel(0);

        let (server_conn_request_sender, server_conn_request_receiver) = mpsc::channel(0);
        let server_connector = DummyConnector::new(server_conn_request_sender);

        let (tick_sender, timer_stream) = mpsc::channel::<()>(0);

        let (graph_requests_sender, graph_requests_receiver) = mpsc::channel(0);
        let graph_client = GraphClient::new(graph_requests_sender);

        let compare_public_key = |pk_a: &PublicKey, pk_b: &PublicKey| pk_a.cmp(pk_b);

        let rng = DummyRandom::new(&[0x13, 0x37, server_public_key[0]]);
        let verifier = SimpleVerifier::new(8, rng);

        let c_server_public_key = server_public_key.clone();

        let (debug_event_sender, debug_event_receiver) = mpsc::channel(0);

        let server_loop_fut = server_loop(
            local_public_key,
            trusted_servers,
            incoming_server_connections,
            incoming_client_connections,
            server_connector,
            graph_client,
            compare_public_key,
            verifier,
            timer_stream,
            spawner.clone(),
            Some(debug_event_sender),
        )
        .map_err(move |e| {
            error!(
                "Error in server_loop() for pk[0] = {}: {:?}",
                c_server_public_key[0], e
            )
        })
        .map(|_| ());

        spawner.spawn(server_loop_fut).unwrap();

        TestServer {
            public_key: server_public_key.clone(),
            tick_sender,
            server_connections_sender,
            client_connections_sender,
            graph_requests_receiver,
            server_conn_request_receiver,
            debug_event_receiver,
        }
    }

    async fn handle_connect(test_servers: &mut [TestServer], from_index: usize) {
        let conn_request =
            await!(test_servers[from_index].server_conn_request_receiver.next()).unwrap();
        let (a_sender, b_receiver) = mpsc::channel(CHANNEL_SIZE);
        let (b_sender, a_receiver) = mpsc::channel(CHANNEL_SIZE);

        let (_dest_public_key, dest_index) = conn_request.address.clone();
        await!(test_servers[dest_index as usize]
            .server_connections_sender
            .send((
                test_servers[from_index].public_key.clone(),
                (b_sender, b_receiver)
            )))
        .unwrap();

        await!(test_servers[dest_index as usize]
            .debug_event_receiver
            .next())
        .unwrap();

        conn_request.reply(Some((a_sender, a_receiver)));
        await!(test_servers[from_index].debug_event_receiver.next()).unwrap();
    }

    async fn task_index_server_loop_multi_server<S>(spawner: S)
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

        let mut test_servers = Vec::new();
        test_servers.push(create_test_server(0, &[1, 2], spawner.clone()));
        test_servers.push(create_test_server(1, &[0, 3], spawner.clone()));
        test_servers.push(create_test_server(2, &[0, 3], spawner.clone()));
        test_servers.push(create_test_server(3, &[1, 2, 4], spawner.clone()));
        test_servers.push(create_test_server(4, &[3], spawner.clone()));

        // Let all servers connect:
        await!(handle_connect(&mut test_servers[..], 4)); // 4 connects to {3}
        await!(handle_connect(&mut test_servers[..], 3)); // 3 connects to {1,2}
        await!(handle_connect(&mut test_servers[..], 3));
        await!(handle_connect(&mut test_servers[..], 2)); // 2 connects to {0}
        await!(handle_connect(&mut test_servers[..], 1)); // 1 connects to {0}

        // Let some time pass, to fill server's time hash lists.
        // Without this step it will not be possible to forward messages along long routes.
        for _iter in 0..32usize {
            await!(test_servers[0].tick_sender.send(())).unwrap();
            await!(test_servers[0].debug_event_receiver.next()).unwrap();
            for &j in &[1usize, 2] {
                await!(test_servers[j].debug_event_receiver.next()).unwrap();
            }

            await!(test_servers[1].tick_sender.send(())).unwrap();
            await!(test_servers[1].debug_event_receiver.next()).unwrap();
            for &j in &[0usize, 3] {
                await!(test_servers[j].debug_event_receiver.next()).unwrap();
            }

            await!(test_servers[2].tick_sender.send(())).unwrap();
            await!(test_servers[2].debug_event_receiver.next()).unwrap();
            for &j in &[0usize, 3] {
                await!(test_servers[j].debug_event_receiver.next()).unwrap();
            }

            await!(test_servers[3].tick_sender.send(())).unwrap();
            await!(test_servers[3].debug_event_receiver.next()).unwrap();
            for &j in &[1usize, 2, 4] {
                await!(test_servers[j].debug_event_receiver.next()).unwrap();
            }

            await!(test_servers[4].tick_sender.send(())).unwrap();
            await!(test_servers[4].debug_event_receiver.next()).unwrap();
            for &j in &[3usize] {
                await!(test_servers[j].debug_event_receiver.next()).unwrap();
            }
        }

        // Connect a client to server 0:
        let identity_client = create_identity_client(spawner.clone(), &[1, 1]);
        let client_public_key = await!(identity_client.request_public_key()).unwrap();

        let (mut client_sender, server_receiver) = mpsc::channel(CHANNEL_SIZE);
        let (server_sender, mut client_receiver) = mpsc::channel(CHANNEL_SIZE);
        await!(test_servers[0]
            .client_connections_sender
            .send((client_public_key.clone(), (server_sender, server_receiver))))
        .unwrap();
        await!(test_servers[0].debug_event_receiver.next()).unwrap();

        // Client requests routes: We do this to make sure the new client is registered at the
        // server before we send more time ticks. This will ensure that the server gets
        // ClientConnected event before the TimeTick event:
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
        match await!(test_servers[0].graph_requests_receiver.next()).unwrap() {
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
                assert!(response_routes.multi_routes.is_empty());
            }
            _ => unreachable!(),
        };

        // One time iteration for server 0:
        await!(test_servers[0].tick_sender.send(())).unwrap();
        await!(test_servers[0].debug_event_receiver.next()).unwrap();
        for &j in &[1usize, 2] {
            await!(test_servers[j].debug_event_receiver.next()).unwrap();
        }

        // Get time hash sent to the new client:
        let time_hash0 = match await!(client_receiver.next()).unwrap() {
            IndexServerToClient::TimeHash(time_hash) => time_hash,
            _ => unreachable!(),
        };

        // Send mutations update to the server:
        let index_mutations = vec![IndexMutation::RemoveFriend(PublicKey::from(
            &[11; PUBLIC_KEY_LEN],
        ))];

        let mut mutations_update = MutationsUpdate {
            node_public_key: client_public_key.clone(),
            index_mutations,
            time_hash: time_hash0.clone(),
            session_id: Uid::from(&[0; UID_LEN]),
            counter: 0,
            rand_nonce: RandValue::from(&[0; RAND_VALUE_LEN]),
            signature: Signature::from(&[0; SIGNATURE_LEN]),
        };

        // Calculate signature:
        mutations_update.signature =
            await!(identity_client.request_signature(mutations_update.signature_buff().clone()))
                .unwrap();

        await!(client_sender.send(IndexClientToServer::MutationsUpdate(mutations_update))).unwrap();

        macro_rules! process_graph_request {
            ($index:expr) => {
                // Handle tick request:
                match await!(test_servers[$index].graph_requests_receiver.next()).unwrap() {
                    GraphRequest::Tick(node, response_sender) => {
                        assert_eq!(node, client_public_key);
                        response_sender.send(()).unwrap();
                    }
                    _ => unreachable!(),
                }

                match await!(test_servers[$index].graph_requests_receiver.next()).unwrap() {
                    GraphRequest::RemoveEdge(src, dest, response_sender) => {
                        assert_eq!(src, client_public_key);
                        assert_eq!(dest, PublicKey::from(&[11; PUBLIC_KEY_LEN]));
                        response_sender.send(None).unwrap();
                    }
                    _ => unreachable!(),
                };
                await!(test_servers[$index].debug_event_receiver.next()).unwrap();
            };
        }

        /*
         *  Servers layout:
         *
         *    0 -- 1
         *    |    |
         *    2 -- 3 -- 4
         */

        process_graph_request!(0);
        process_graph_request!(1);
        process_graph_request!(3);
        process_graph_request!(2);
        process_graph_request!(4);
    }

    #[test]
    fn test_index_server_loop_multi_server() {
        // Example of how to run with logs:
        // RUST_LOG=offst_index_server=info cargo test -p offst-index-server multi  -- --nocapture
        let _ = env_logger::init();
        let mut thread_pool = ThreadPool::new().unwrap();
        thread_pool.run(task_index_server_loop_multi_server(thread_pool.clone()));
    }

    // TODO: Add tests.
}
