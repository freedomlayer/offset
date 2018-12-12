use std::cmp::Ordering;
use std::collections::HashMap;

use futures::{select, FutureExt, StreamExt, SinkExt};
use futures::channel::{mpsc, oneshot};
use futures::task::{Spawn, SpawnExt};

use common::conn::{ConnPair, Listener, FutTransform};
use crypto::identity::{PublicKey, compare_public_key};

use timer::TimerClient;

use proto::index::messages::{IndexServerToClient, 
    IndexClientToServer, IndexServerToServer};

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


struct IndexServer<A,S,SC> {
    local_public_key: PublicKey,
    server_connector: SC,
    remote_servers: HashMap<PublicKey, RemoteServer<A>>,
    event_sender: mpsc::Sender<IndexServerEvent>,
    spawner: S,
}


#[derive(Debug)]
pub enum IndexServerError {
    SpawnError,
}

#[derive(Debug)]
enum IndexServerEvent {
    ServerConnection((PublicKey, ServerConn)),
    FromServer((PublicKey, Option<IndexServerToServer>)),
    ClientConnection((PublicKey, ClientConn)),
    FromClient((PublicKey, Option<IndexClientToServer>))
}



impl<A,S,SC> IndexServer<A,S,SC> 
where
    A: Clone + Send + 'static,
    S: Spawn + Send, 
    SC: FutTransform<Input=(PublicKey, A), Output=ServerConn> + Clone + Send + 'static,
{
    pub fn new(index_server_config: IndexServerConfig<A>,
               server_connector: SC,
               event_sender: mpsc::Sender<IndexServerEvent>,
               spawner: S) -> Result<IndexServer<A,S,SC>, IndexServerError> {

        let IndexServerConfig {local_public_key, trusted_servers} 
                = index_server_config;

        let mut index_server = IndexServer {
            local_public_key,
            server_connector,
            remote_servers: HashMap::new(),
            event_sender,
            spawner,
        };

        let mut remote_servers: HashMap<PublicKey, RemoteServer<A>> = HashMap::new();
        for (public_key, address) in trusted_servers.into_iter() {
            let remote_server = index_server.spawn_server(public_key.clone(), address)?;
            index_server.remote_servers.insert(public_key, remote_server);
        }
        Ok(index_server)
    }

    fn is_listen_friend(&self, friend_public_key: &PublicKey) -> bool {
        compare_public_key(&self.local_public_key, friend_public_key) == Ordering::Less
    }

    fn spawn_server(&mut self, public_key: PublicKey, address: A) 
        -> Result<RemoteServer<A>, IndexServerError> {

        if self.is_listen_friend(&public_key) {
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
}


async fn server_loop<A,SL,SC,CL,S>(index_server_config: IndexServerConfig<A>,
                                 server_listener: SL,
                                 server_connector: SC,
                                 client_listener: CL,
                                 timer_client: TimerClient,
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


    let (event_sender, event_receiver) = mpsc::channel(0);

    let mut index_server = IndexServer::new(
               index_server_config,
               server_connector,
               event_sender,
               spawner);

    let incoming_server_connections = incoming_server_connections
        .map(|server_connection| IndexServerEvent::ServerConnection(server_connection));

    let incoming_client_connections = incoming_client_connections
        .map(|client_connection| IndexServerEvent::ClientConnection(client_connection));

    let mut events = event_receiver
        .select(incoming_server_connections)
        .select(incoming_client_connections);

    while let Some(event) = await!(events.next()) {
        match event {
            IndexServerEvent::ServerConnection((public_key, server_conn)) => {},
            IndexServerEvent::FromServer((public_key, Some(index_server_to_server))) => {},
            IndexServerEvent::FromServer((public_key, None)) => {},
            IndexServerEvent::ClientConnection((public_key, client_conn)) => {},
            IndexServerEvent::FromClient((public_key, Some(index_client_to_server))) => {},
            IndexServerEvent::FromClient((public_key, None)) => {},
        }
        // TODO
    }
    unimplemented!();
}


