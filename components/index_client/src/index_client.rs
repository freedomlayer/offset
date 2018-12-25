use std::marker::Unpin;
use std::collections::VecDeque;

use futures::{future, FutureExt, stream, Stream, StreamExt, Sink, SinkExt};
use futures::channel::mpsc;
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
struct ServerConnected<ISA> {
    address: ISA,
    opt_control_sender: Option<ControlSender>,
}

enum ConnStatus<ISA> {
    Empty,
    Connecting(ISA),
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
    ISA: Clone + Send + 'static,
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

    pub async fn handle_from_app_server(&mut self, 
                                  app_server_to_index_client: AppServerToIndexClient<ISA>) 
                                    -> Result<(), IndexClientError> {

        match app_server_to_index_client {
            AppServerToIndexClient::AddIndexServer(address) => unimplemented!(),
            AppServerToIndexClient::RemoveIndexServer(address) => unimplemented!(),
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
            ConnStatus::Connecting(address) => address,
        };

        self.conn_status = ConnStatus::Connected(ServerConnected {
            address: address.clone(),
            opt_control_sender: Some(control_sender)
        });
    }

    pub fn handle_index_server_closed(&mut self) -> Result<(), IndexClientError> {
        let server_address = match self.index_servers.pop_front() {
            Some(server_address) => server_address,
            None => {
                // We don't have any inde servers to connect to:
                self.conn_status = ConnStatus::Empty;
                return Ok(());
            }
        };
        // Move the chosen address to the end, rotating the addresses VecDeque 1 to the left:
        self.index_servers.push_back(server_address.clone());

        let mut c_event_sender = self.event_sender.clone();
        let mut c_index_client_session = self.index_client_session.clone();

        self.conn_status = ConnStatus::Connecting(server_address.clone());

        let connect_fut = async move {
            match await!(c_index_client_session.transform(server_address)) {
                Some((control_sender, close_receiver)) => {
                    let _ = await!(c_event_sender.send(IndexClientEvent::IndexServerConnected(control_sender)));
                    let _ = await!(close_receiver);
                    let _ = await!(c_event_sender.send(IndexClientEvent::IndexServerClosed));
                },
                None => {
                    // TODO: Should we report an error here somehow if send fails?
                    let _ = await!(c_event_sender.send(IndexClientEvent::IndexServerClosed));
                },
            };
        };

        self.spawner.spawn(connect_fut)
            .map_err(|_| IndexClientError::SpawnError)
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
    ISA: Clone + Send + 'static,
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
