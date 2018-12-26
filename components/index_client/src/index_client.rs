use std::marker::Unpin;
use std::collections::VecDeque;

use futures::{select, future, FutureExt, stream, Stream, StreamExt, Sink, SinkExt};
use futures::channel::{mpsc, oneshot};
use futures::task::{Spawn, SpawnExt};

use common::conn::FutTransform;

use crypto::identity::PublicKey;
use crypto::uid::Uid;

use proto::index_client::messages::{AppServerToIndexClient, IndexClientToAppServer,
                                    IndexClientMutation, IndexClientState,
                                    ResponseRoutesResult, ClientResponseRoutes};

use timer::TimerClient;

use crate::client_session::{SessionHandle, ControlSender, CloseReceiver};

pub struct IndexClientConfig<ISA> {
    index_servers: Vec<ISA>,
}

pub enum IndexClientConfigMutation<ISA> {
    AddIndexServer(ISA),
    RemoveIndexServer(ISA),
}

#[derive(Debug)]
struct ServerConnecting<ISA> {
    address: ISA,
    opt_cancel_sender: Option<oneshot::Sender<()>>,
}


#[derive(Debug)]
struct ServerConnected<ISA> {
    address: ISA,
    opt_control_sender: Option<ControlSender>,
}

#[derive(Debug)]
enum ConnStatus<ISA> {
    Empty,
    Connecting(ServerConnecting<ISA>),
    Connected(ServerConnected<ISA>),
}

#[derive(Debug)]
pub enum IndexClientError {
    AppServerClosed,
    SendToAppServerFailed,
    // SendIndexClientEventFailed,
    SpawnError,
}

#[derive(Debug)]
enum IndexClientEvent<ISA> {
    FromAppServer(AppServerToIndexClient<ISA>),
    AppServerClosed,
    IndexServerConnected(ControlSender),
    IndexServerClosed,
    ResponseRoutes((Uid, ResponseRoutesResult)),
}

struct IndexClient<ISA,TAS,ICS,DB,S> {
    event_sender: mpsc::Sender<IndexClientEvent<ISA>>,
    to_app_server: TAS,
    /// A cyclic list of index server addresses.
    /// The next index server to be used is the one on the front:
    index_servers: VecDeque<ISA>,
    index_client_state: IndexClientState,
    index_client_session: ICS,
    max_open_requests: usize,
    num_open_requests: usize,
    conn_status: ConnStatus<ISA>,
    database: DB,
    timer_client: TimerClient,
    spawner: S,
}

impl<ISA,TAS,ICS,DB,S> IndexClient<ISA,TAS,ICS,DB,S> 
where
    ISA: Eq + Clone + Send + 'static,
    TAS: Sink<SinkItem=IndexClientToAppServer<ISA>> + Unpin,
    ICS: FutTransform<Input=ISA, Output=Option<SessionHandle>> + Clone + Send + 'static,
    DB: FutTransform<Input=IndexClientConfigMutation<ISA>, Output=Option<()>>,
    S: Spawn,
{
    pub fn new(event_sender: mpsc::Sender<IndexClientEvent<ISA>>,
               to_app_server: TAS,
               index_client_config: IndexClientConfig<ISA>,
               index_client_state: IndexClientState,
               index_client_session: ICS,
               max_open_requests: usize,
               database: DB,
               timer_client: TimerClient,
               spawner: S) -> Self { 

        let index_servers = index_client_config.index_servers
                .into_iter()
                .collect::<VecDeque<_>>();

        IndexClient {
            event_sender,
            to_app_server,
            index_servers,
            index_client_state,
            index_client_session,
            max_open_requests,
            num_open_requests: 0,
            conn_status: ConnStatus::Empty,
            database,
            timer_client,
            spawner,
        }
    }

    /// Attempt to connect to server.
    /// If there are no index servers known, do nothing.
    fn try_connect_to_server(&mut self) -> Result<(), IndexClientError> {
        // Make sure that conn_status is empty:
        if let ConnStatus::Empty = self.conn_status {
        } else {
            unreachable!();
        }

        let server_address = match self.index_servers.pop_front() {
            Some(server_address) => server_address,
            None => {
                // We don't have any index servers to connect to:
                self.conn_status = ConnStatus::Empty;
                return Ok(());
            }
        };
        // Move the chosen address to the end, rotating the addresses VecDeque 1 to the left:
        self.index_servers.push_back(server_address.clone());

        let mut c_index_client_session = self.index_client_session.clone();
        let mut c_event_sender = self.event_sender.clone();

        let (cancel_sender, cancel_receiver) = oneshot::channel();
        let server_connecting = ServerConnecting {
            address: server_address.clone(),
            opt_cancel_sender: Some(cancel_sender),
        };
        self.conn_status = ConnStatus::Connecting(server_connecting);

        // TODO: Can we remove the Box::pinned() from here? How?
        let connect_fut = Box::pinned(async move {
            await!(c_index_client_session.transform(server_address))
        });

        let cancellable_fut = async move {
            // During the connection stage it is possible to cancel using the `cancel_sender`:
            let select_res = select! {
                connect_fut = connect_fut.fuse() => connect_fut,
                _cancel_receiver = cancel_receiver.fuse() => None,
            };

            // After the connection stage it is possible to close the connection by closing the
            // control_sender:
            if let Some((control_sender, close_receiver)) = select_res {
                let _ = await!(c_event_sender.send(IndexClientEvent::IndexServerConnected(control_sender)));
                let _ = await!(close_receiver);
            }
            let _ = await!(c_event_sender.send(IndexClientEvent::IndexServerClosed));
        };

        self.spawner.spawn(cancellable_fut)
            .map_err(|_| IndexClientError::SpawnError)
    }

    pub async fn handle_from_app_server(&mut self, 
                                  app_server_to_index_client: AppServerToIndexClient<ISA>) 
                                    -> Result<(), IndexClientError> {

        match app_server_to_index_client {
            AppServerToIndexClient::AddIndexServer(address) => {
                // Update database:
                await!(self.database.transform(IndexClientConfigMutation::AddIndexServer(address.clone())));

                self.index_servers.push_back(address);
                if let ConnStatus::Empty = self.conn_status {
                } else {
                    return Ok(());
                }
                // We are here if conn_status is empty.
                // This means we should initiate connection to an index server
                self.try_connect_to_server()?
            },
            AppServerToIndexClient::RemoveIndexServer(address) => {
                // Update database:
                await!(self.database.transform(IndexClientConfigMutation::RemoveIndexServer(address.clone())));

                // Remove address:
                self.index_servers.retain(|cur_address| cur_address != &address);

                // Disconnect a current server connection if it uses the removed address:
                match &mut self.conn_status {
                    ConnStatus::Empty => {}, // Nothing to do here
                    ConnStatus::Connecting(server_connecting) => {
                        if server_connecting.address == address {
                            if let Some(cancel_sender) = server_connecting.opt_cancel_sender.take() {
                                let _ = cancel_sender.send(());
                            }
                        }
                    },
                    ConnStatus::Connected(server_connected) => {
                        server_connected.opt_control_sender.take();
                    },
                }
            },
            AppServerToIndexClient::RequestRoutes(request_routes) => unimplemented!(),
            AppServerToIndexClient::ApplyMutations(mutations) => unimplemented!(),
        }
        unimplemented!();
    }

    pub fn handle_index_server_connected(&mut self, control_sender: ControlSender) {
        let address = match &self.conn_status {
            ConnStatus::Empty => {
                error!("Did not attempt to connect!");
                return;
            },
            ConnStatus::Connected(_) => {
                error!("Already connected to server!");
                return;
            }
            ConnStatus::Connecting(server_connecting) => &server_connecting.address,
        };

        self.conn_status = ConnStatus::Connected(ServerConnected {
            address: address.clone(),
            opt_control_sender: Some(control_sender)
        });
    }


    pub fn handle_index_server_closed(&mut self) -> Result<(), IndexClientError> {
        self.conn_status = ConnStatus::Empty;
        self.try_connect_to_server()
    }

    pub async fn handle_response_routes(&mut self, request_id: Uid, 
                                        response_routes_result: ResponseRoutesResult) 
                                            -> Result<(), IndexClientError> {

        self.num_open_requests = self.num_open_requests.checked_sub(1).unwrap();

        let client_response_routes = ClientResponseRoutes {
            request_id,
            result: response_routes_result,
        };

        await!(self.to_app_server.send(IndexClientToAppServer::ResponseRoutes(client_response_routes)))
            .map_err(|_| IndexClientError::SendToAppServerFailed)
    }
}

pub async fn index_client_loop<ISA,FAS,TAS,ICS,DB,S>(from_app_server: FAS,
                               to_app_server: TAS,
                               mut index_client_config: IndexClientConfig<ISA>,
                               index_client_state: IndexClientState,
                               index_client_session: ICS,
                               max_open_requests: usize,
                               database: DB,
                               timer_client: TimerClient,
                               spawner: S) -> Result<(), IndexClientError>
where
    ISA: Eq + Clone + Send + 'static,
    FAS: Stream<Item=AppServerToIndexClient<ISA>> + Unpin,
    TAS: Sink<SinkItem=IndexClientToAppServer<ISA>> + Unpin,
    ICS: FutTransform<Input=ISA, Output=Option<SessionHandle>> + Clone + Send + 'static,
    DB: FutTransform<Input=IndexClientConfigMutation<ISA>, Output=Option<()>>,
    S: Spawn,
{
    let (event_sender, event_receiver) = mpsc::channel(0);
    let mut index_client = IndexClient::new(event_sender, 
                                            to_app_server,
                                            index_client_config,
                                            index_client_state,
                                            index_client_session,
                                            max_open_requests, 
                                            database,
                                            timer_client,
                                            spawner);
    
    index_client.try_connect_to_server()?;

    let from_app_server = from_app_server
        .map(|app_server_to_index_client| IndexClientEvent::FromAppServer(app_server_to_index_client))
        .chain(stream::once(future::ready(IndexClientEvent::AppServerClosed)));

    let mut events = event_receiver
        .select(from_app_server);

    while let Some(event) = await!(events.next()) {
        match event {
            IndexClientEvent::FromAppServer(app_server_to_index_client) =>
                await!(index_client.handle_from_app_server(app_server_to_index_client))?,
            IndexClientEvent::AppServerClosed => return Err(IndexClientError::AppServerClosed),
            IndexClientEvent::IndexServerConnected(control_sender) => 
                index_client.handle_index_server_connected(control_sender),
            IndexClientEvent::IndexServerClosed =>
                index_client.handle_index_server_closed()?,
            IndexClientEvent::ResponseRoutes((request_id, response_routes_result)) =>
                await!(index_client.handle_response_routes(request_id, response_routes_result))?,
        };
    }
    Ok(())
}
