use std::marker::Unpin;
use std::collections::{VecDeque, HashSet};

use futures::{select, future, FutureExt, stream, Stream, StreamExt, Sink, SinkExt};
use futures::channel::{mpsc, oneshot};
use futures::task::{Spawn, SpawnExt};

use common::conn::FutTransform;

use crypto::identity::PublicKey;
use crypto::uid::Uid;

use proto::index_client::messages::{AppServerToIndexClient, IndexClientToAppServer,
                                    IndexMutation, IndexClientState,
                                    ResponseRoutesResult, ClientResponseRoutes,
                                    RequestRoutes, UpdateFriend};

use timer::TimerClient;

use crate::client_session::{SessionHandle, ControlSender, CloseReceiver};
use crate::single_client::SingleClientControl;

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

struct SeqIndexClientState {
    index_client_state: IndexClientState,
    friends_queue: VecDeque<PublicKey>,
}

fn apply_index_mutation(index_client_state: &mut IndexClientState, 
                        index_mutation: &IndexMutation) {
    match index_mutation {
        IndexMutation::UpdateFriend(update_friend) => {
            let capacity_pair = (update_friend.send_capacity, update_friend.recv_capacity);
            let _ = index_client_state.friends.insert(update_friend.public_key.clone(), capacity_pair.clone());
        }
        IndexMutation::RemoveFriend(public_key) => {
            let _ = index_client_state.friends.remove(public_key);
        },
    }
}

impl SeqIndexClientState {
    pub fn new(index_client_state: IndexClientState) -> Self {
        let friends_queue = index_client_state.friends
            .iter()
            .map(|(public_key, _)| public_key.clone())
            .collect::<VecDeque<_>>();

        SeqIndexClientState {
            index_client_state,
            friends_queue,
        }
    }

    pub fn mutate(&mut self, index_mutation: &IndexMutation) {
        apply_index_mutation(&mut self.index_client_state, index_mutation);

        match index_mutation {
            IndexMutation::UpdateFriend(update_friend) => {
                // Put the newly updated friend in the end of the queue:
                // TODO: Possibly optimize this later. Might be slow:
                self.friends_queue.retain(|public_key| public_key != &update_friend.public_key);
                self.friends_queue.push_back(update_friend.public_key.clone());
            },
            IndexMutation::RemoveFriend(friend_public_key) => {
                // Remove from queue:
                // TODO: Possibly optimize this later. Might be slow:
                self.friends_queue.retain(|public_key| public_key != friend_public_key);
            },
        }
    }

    /// Return information of some current friend.
    ///
    /// Should return all friends after about n calls, where n is the amount of friends.
    /// This is important as the index server relies on this behaviour. If some friend is not
    /// returned after a large amount of calls, it will be deleted from the server.
    pub fn next_friend(&mut self) -> Option<UpdateFriend> {
        match self.friends_queue.pop_front() {
            Some(friend_public_key) => {
                // Move to the end of the queue:
                self.friends_queue.push_back(friend_public_key.clone());

                let (send_capacity, recv_capacity) = 
                    self.index_client_state.friends.get(&friend_public_key)
                        .unwrap()
                        .clone();

                Some(UpdateFriend {
                    public_key: friend_public_key,
                    send_capacity,
                    recv_capacity,
                })
            },
            None => None,
        }
    }
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

    pub async fn return_response_routes_failure(&mut self, request_id: Uid) 
                                            -> Result<(), IndexClientError> {

        let client_response_routes = ClientResponseRoutes {
            request_id,
            result: ResponseRoutesResult::Failure,
        };
        return await!(self.to_app_server.send(IndexClientToAppServer::ResponseRoutes(client_response_routes)))
            .map_err(|_| IndexClientError::SendToAppServerFailed)
    }

    pub async fn handle_from_app_server_add_index_server(&mut self, server_address: ISA) 
                                                            -> Result<(), IndexClientError> {
        // Update database:
        await!(self.database.transform(IndexClientConfigMutation::AddIndexServer(server_address.clone())));

        self.index_servers.push_back(server_address);
        if let ConnStatus::Empty = self.conn_status {
        } else {
            return Ok(());
        }
        // We are here if conn_status is empty.
        // This means we should initiate connection to an index server
        self.try_connect_to_server()
    }

    pub async fn handle_from_app_server_remove_index_server(&mut self, server_address: ISA) 
                                                            -> Result<(), IndexClientError> {
        // Update database:
        await!(self.database.transform(IndexClientConfigMutation::RemoveIndexServer(server_address.clone())));

        // Remove address:
        self.index_servers.retain(|cur_address| cur_address != &server_address);

        // Disconnect a current server connection if it uses the removed address:
        match &mut self.conn_status {
            ConnStatus::Empty => {}, // Nothing to do here
            ConnStatus::Connecting(server_connecting) => {
                if server_connecting.address == server_address {
                    if let Some(cancel_sender) = server_connecting.opt_cancel_sender.take() {
                        let _ = cancel_sender.send(());
                    }
                }
            },
            ConnStatus::Connected(server_connected) => {
                server_connected.opt_control_sender.take();
            },
        }
        Ok(())
    }

    pub async fn handle_from_app_server_request_routes(&mut self, request_routes: RequestRoutes) 
                                                                -> Result<(), IndexClientError> {
        if self.num_open_requests >= self.max_open_requests {
            return await!(self.return_response_routes_failure(request_routes.request_id));
        }

        // Check server connection status:
        let mut server_connected = match &mut self.conn_status {
            ConnStatus::Empty |
            ConnStatus::Connecting(_) => return await!(self.return_response_routes_failure(request_routes.request_id)),
            ConnStatus::Connected(server_connected) => server_connected,
        };

        let mut control_sender = match server_connected.opt_control_sender.take() {
            Some(control_sender) => control_sender,
            None => return await!(self.return_response_routes_failure(request_routes.request_id)),
        };
        
        let c_request_id = request_routes.request_id.clone();
        let (response_sender, response_receiver) = oneshot::channel();
        let single_client_control = SingleClientControl::RequestRoutes((request_routes, response_sender));

        match await!(control_sender.send(single_client_control)) {
            Ok(()) => server_connected.opt_control_sender = Some(control_sender),
            Err(_) => return await!(self.return_response_routes_failure(c_request_id)),
        };

        let mut c_event_sender = self.event_sender.clone();
        let request_fut = async move {
            let response_routes_result = match await!(response_receiver) {
                Ok(routes) => ResponseRoutesResult::Success(routes),
                Err(_) => ResponseRoutesResult::Failure,
            };
            // TODO: Should report error here if failure occurs?
            let _ = await!(c_event_sender.send(IndexClientEvent::ResponseRoutes((c_request_id, response_routes_result))));
        };

        self.num_open_requests = self.num_open_requests.saturating_add(1);
        self.spawner.spawn(request_fut)
            .map_err(|_| IndexClientError::SpawnError)
    }

    pub async fn handle_from_app_server_apply_mutations(&mut self, mut mutations: Vec<IndexMutation>) 
                                                                        -> Result<(), IndexClientError> {
        // Update state:
        for mutation in &mutations {
            apply_index_mutation(&mut self.index_client_state, mutation);
        }

        // Check if server is ready:
        let server_connected = match &mut self.conn_status {
            ConnStatus::Empty |
            ConnStatus::Connecting(_) => return Ok(()), // Server is not ready
            ConnStatus::Connected(server_connected) => server_connected,
        };

        let mut control_sender = match server_connected.opt_control_sender.take() {
            Some(control_sender) => control_sender,
            None => return Ok(()),
        };

        // TODO: 
        // - Append to mutations a state of a friend chosen sequentially (or randomly?)
        panic!("Not done implementing");

        if let Ok(()) = await!(control_sender.send(SingleClientControl::SendMutations(mutations))) {
            server_connected.opt_control_sender = Some(control_sender);
        }

        Ok(())
    }

    pub async fn handle_from_app_server(&mut self, 
                                  app_server_to_index_client: AppServerToIndexClient<ISA>) 
                                    -> Result<(), IndexClientError> {

        match app_server_to_index_client {
            AppServerToIndexClient::AddIndexServer(server_address) => 
                await!(self.handle_from_app_server_add_index_server(server_address)),
            AppServerToIndexClient::RemoveIndexServer(server_address) =>
                await!(self.handle_from_app_server_remove_index_server(server_address)),
            AppServerToIndexClient::RequestRoutes(request_routes) =>
                await!(self.handle_from_app_server_request_routes(request_routes)),
            AppServerToIndexClient::ApplyMutations(mutations) =>
                await!(self.handle_from_app_server_apply_mutations(mutations)),
        }
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
