use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};

use futures::{select, future, FutureExt, TryFutureExt, stream, StreamExt, SinkExt};
use futures::channel::{mpsc, oneshot};
use futures::task::{Spawn, SpawnExt};

use common::conn::{ConnPair, Listener, FutTransform};
use crypto::identity::{PublicKey, compare_public_key};

use timer::TimerClient;

use proto::index::messages::{IndexServerToClient, 
    IndexClientToServer, IndexServerToServer, 
    ResponseRoutes, RouteWithCapacity, MutationsUpdate,
    ForwardMutationsUpdate};

use proto::funder::messages::FriendsRoute;

use crate::graph::graph_service::{GraphClient, GraphClientError};


type ServerConn = ConnPair<IndexServerToServer, IndexServerToServer>;
type ClientConn = ConnPair<IndexServerToClient, IndexClientToServer>;


struct IndexServerConfig<A> {
    local_public_key: PublicKey,
    // map: public_key -> server_address
    trusted_servers: HashMap<PublicKey, A>, 
}

struct ServerConnected {
    opt_sender: Option<mpsc::Sender<IndexServerToServer>>,
}

struct ServerInitiating {
    #[allow(unused)]
    close_sender: oneshot::Sender<()>,
}


enum RemoteServerState {
    Connected(ServerConnected),
    Initiating(ServerInitiating),
    Listening,
}

struct RemoteServer<A> {
    address: A,
    state: RemoteServerState,
}

/*
struct Client {
    opt_sender: Option<mpsc::Sender<IndexServerToClient>>,
}
*/

struct IndexServer<A,S,SC> {
    local_public_key: PublicKey,
    server_connector: SC,
    graph_client: GraphClient<PublicKey,u128>,
    remote_servers: HashMap<PublicKey, RemoteServer<A>>,
    clients: HashSet<PublicKey>,
    event_sender: mpsc::Sender<IndexServerEvent>,
    spawner: S,
}


#[derive(Debug)]
pub enum IndexServerError {
    SpawnError,
    RequestTimerStreamFailed,
    GraphClientError,
    ClientSenderError,
    ClientEventSenderError,
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



impl<A,S,SC> IndexServer<A,S,SC> 
where
    A: Clone + Send + 'static,
    S: Spawn + Send, 
    SC: FutTransform<Input=(PublicKey, A), Output=ServerConn> + Clone + Send + 'static,
{
    pub fn new(index_server_config: IndexServerConfig<A>,
               server_connector: SC,
               graph_client: GraphClient<PublicKey, u128>,
               event_sender: mpsc::Sender<IndexServerEvent>,
               spawner: S) -> Result<IndexServer<A,S,SC>, IndexServerError> {

        let IndexServerConfig {local_public_key, trusted_servers} 
                = index_server_config;

        let mut index_server = IndexServer {
            local_public_key,
            server_connector,
            graph_client,
            remote_servers: HashMap::new(),
            clients: HashSet::new(),
            event_sender,
            spawner,
        };

        for (public_key, address) in trusted_servers.into_iter() {
            let remote_server = index_server.spawn_server(public_key.clone(), address)?;
            index_server.remote_servers.insert(public_key, remote_server);
        }
        Ok(index_server)
    }

    /// We divide servers into two types:
    /// 1. "listen server": A trusted server which has the responsibility of connecting to us.
    /// 2. "init server": A trusted server for which we have the responsibility to initiate
    ///    connection to.
    fn is_listen_server(&self, friend_public_key: &PublicKey) -> bool {
        compare_public_key(&self.local_public_key, friend_public_key) == Ordering::Less
    }

    pub fn spawn_server(&mut self, public_key: PublicKey, address: A) 
        -> Result<RemoteServer<A>, IndexServerError> {

        if self.is_listen_server(&public_key) {
            return Ok(RemoteServer {
                address,
                state: RemoteServerState::Listening,
            })
        }

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
                                                 forward_mutations_update: ForwardMutationsUpdate)
                                                    -> Result<(), IndexServerError> 
    {
        // TODO:
        // - Verify signature.
        // - Verify time hashes chain
        // - If a new session: overwrite previous session.
        // - If counter is greater: increase counter
        // - Return mutations
        // - Apply mutations using graph client.
        // - Forward to all servers
        //      - What if can't forward to some server? Drop the message?
        //      - Where do we buffer server messages? (In connector / listener implementation?)
        unimplemented!();
    }

    pub async fn handle_from_server(&mut self, public_key: PublicKey, server_msg: IndexServerToServer)
        -> Result<(), IndexServerError> {

        match server_msg {
            IndexServerToServer::TimeHash(hash) => {},
            IndexServerToServer::ForwardMutationsUpdate(forward_mutations_update) => {
                await!(self.handle_forward_mutations_update(Some(public_key), forward_mutations_update))?;
            },
        }

        unimplemented!();
    }

    pub async fn handle_timer_tick(&mut self)
        -> Result<(), IndexServerError> {

        unimplemented!();
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


async fn server_loop<A,SL,SC,CL,S>(index_server_config: IndexServerConfig<A>,
                                 server_listener: SL,
                                 server_connector: SC,
                                 client_listener: CL,
                                 graph_client: GraphClient<PublicKey, u128>,
                                 mut timer_client: TimerClient,
                                 spawner: S) -> Result<(), IndexServerError>
where
    SL: Listener<Connection=(PublicKey, ServerConn), Config=(), Arg=()>,
    SC: FutTransform<Input=(PublicKey, A), Output=ServerConn> + Clone + Send + 'static,
    CL: Listener<Connection=(PublicKey, ClientConn), Config=(), Arg=()>,
    S: Spawn + Send,
    A: Clone + Send + 'static,
{
    let (server_listener_config_sender, incoming_server_connections) = server_listener.listen(());
    let (client_listener_config_sender, incoming_client_connections) = client_listener.listen(());

    let timer_stream = await!(timer_client.request_timer_stream())
        .map_err(|_| IndexServerError::RequestTimerStreamFailed)?;

    let (event_sender, event_receiver) = mpsc::channel(0);

    let mut index_server = IndexServer::new(
               index_server_config,
               server_connector,
               graph_client,
               event_sender,
               spawner)?;

    let incoming_server_connections = incoming_server_connections
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

                remote_server.state = RemoteServerState::Connected(ServerConnected {
                    opt_sender: Some(sender),
                });

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
                // Server connection closed
                let old_server = match index_server.remote_servers.remove(&public_key) {
                    None => {
                        error!("A non existent server {:?} was closed. Aborting.", public_key);
                        continue;
                    },
                    Some(old_server) => old_server,
                };
                let server = index_server.spawn_server(public_key.clone(), old_server.address)?;
                index_server.remote_servers.insert(public_key, server);

            },
            IndexServerEvent::ClientConnection((public_key, client_conn)) => {
                if index_server.clients.contains(&public_key) {
                    error!("Client {:?} already connected! Aborting.", public_key);
                    continue;
                }

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
                index_server.clients.insert(public_key);
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
                if let false = index_server.clients.remove(&public_key) {
                    error!("A non existent client {:?} was closed.", public_key);
                }
            },
            IndexServerEvent::TimerTick => await!(index_server.handle_timer_tick())?,
        }
        // TODO
    }
    unimplemented!();
}


