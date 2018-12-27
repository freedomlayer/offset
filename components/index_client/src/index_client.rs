use std::marker::Unpin;
use std::collections::{VecDeque, HashSet};

use futures::{select, future, FutureExt, TryFutureExt, 
    stream, Stream, StreamExt, Sink, SinkExt};
use futures::channel::{mpsc, oneshot};
use futures::task::{Spawn, SpawnExt};

use common::conn::FutTransform;

use crypto::identity::PublicKey;
use crypto::uid::Uid;

use proto::index_client::messages::{AppServerToIndexClient, IndexClientToAppServer,
                                    IndexMutation, IndexClientState,
                                    ResponseRoutesResult, ClientResponseRoutes,
                                    RequestRoutes, UpdateFriend,
                                    IndexClientReportMutation};

use crate::client_session::{SessionHandle, ControlSender, CloseReceiver};
use crate::single_client::SingleClientControl;
use crate::seq_friends::SeqFriendsClient;

pub struct IndexClientConfig<ISA> {
    pub index_servers: Vec<ISA>,
}

#[derive(Debug, PartialEq, Eq)]
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
    /// Decrementing counter. When reaches 0 we send a SendMutations
    /// to the server and reset this value to keepalive_ticks:
    ticks_to_send_keepalive: usize,
}

#[derive(Debug)]
enum ConnStatus<ISA> {
    Empty(usize), // ticks_to_reconnect
    Connecting(ServerConnecting<ISA>),
    Connected(ServerConnected<ISA>),
}

#[derive(Debug)]
pub enum IndexClientError {
    AppServerClosed,
    SendToAppServerFailed,
    SpawnError,
    RequestTimerStreamError,
    SeqFriendsError,
    DatabaseError,
}

#[derive(Debug)]
enum IndexClientEvent<ISA> {
    FromAppServer(AppServerToIndexClient<ISA>),
    AppServerClosed,
    IndexServerConnected(ControlSender),
    IndexServerClosed,
    ResponseRoutes((Uid, ResponseRoutesResult)),
    TimerTick,
}


struct IndexClient<ISA,TAS,ICS,DB,S> {
    event_sender: mpsc::Sender<IndexClientEvent<ISA>>,
    to_app_server: TAS,
    /// A cyclic list of index server addresses.
    /// The next index server to be used is the one on the front:
    index_servers: VecDeque<ISA>,
    seq_friends_client: SeqFriendsClient,
    index_client_session: ICS,
    max_open_requests: usize,
    num_open_requests: usize,
    keepalive_ticks: usize,
    backoff_ticks: usize,
    conn_status: ConnStatus<ISA>,
    database: DB,
    spawner: S,
}

/// Send our full friends state as mutations to the server. 
/// We do this in a separate task so that we don't block user requests or incoming funder reports.
async fn send_full_state(mut seq_friends_client: SeqFriendsClient, 
                         mut control_sender: ControlSender) -> Result<(), IndexClientError> {

    await!(seq_friends_client.reset_countdown())
        .map_err(|_| IndexClientError::SeqFriendsError)?;

    loop {
        let next_update_res = await!(seq_friends_client.next_update())
            .map_err(|_| IndexClientError::SeqFriendsError)?;

        let (cyclic_countdown, update_friend) = match next_update_res {
            Some(next_update) => next_update,
            None => break,
        };

        // TODO: Maybe send mutations in batches in the future:
        // However, we need to be careful to not send too many mutations in one batch.
        let mutations = vec![IndexMutation::UpdateFriend(update_friend)];
        if let Err(_) = await!(control_sender.send(SingleClientControl::SendMutations(mutations))) {
            break;
        }
        // Note that here we can not reset the ticks_to_send_keepalive counter because we are
        // running as a separate task. We might want to change this in the future.

        if cyclic_countdown == 0 {
            break;
        }
    }
    Ok(())
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
               seq_friends_client: SeqFriendsClient,
               index_client_session: ICS,
               max_open_requests: usize,
               keepalive_ticks: usize,
               backoff_ticks: usize,
               database: DB,
               spawner: S) -> Self { 

        let index_servers = index_client_config.index_servers
                .into_iter()
                .collect::<VecDeque<_>>();

        IndexClient {
            event_sender,
            to_app_server,
            index_servers,
            seq_friends_client,
            index_client_session,
            max_open_requests,
            num_open_requests: 0,
            keepalive_ticks,
            backoff_ticks,
            conn_status: ConnStatus::Empty(backoff_ticks),
            database,
            spawner,
        }
    }

    /// Attempt to connect to server.
    /// If there are no index servers known, do nothing.
    fn try_connect_to_server(&mut self) -> Result<(), IndexClientError> {
        // Make sure that conn_status is empty:
        if let ConnStatus::Empty(_) = self.conn_status {
        } else {
            unreachable!();
        }

        let server_address = match self.index_servers.pop_front() {
            Some(server_address) => server_address,
            None => {
                // We don't have any index servers to connect to:
                self.conn_status = ConnStatus::Empty(0);
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
        await!(self.database.transform(IndexClientConfigMutation::AddIndexServer(server_address.clone())))
            .ok_or(IndexClientError::DatabaseError)?;

        // Add new server_address to memory:
        self.index_servers.push_back(server_address.clone());

        // Send report:
        let index_client_report_mutation = IndexClientReportMutation::AddIndexServer(server_address);
        let report_mutations = vec![index_client_report_mutation];
        await!(self.to_app_server.send(IndexClientToAppServer::ReportMutations(report_mutations)))
            .map_err(|_| IndexClientError::SendToAppServerFailed)?;

        if let ConnStatus::Empty(_) = self.conn_status {
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
        await!(self.database.transform(IndexClientConfigMutation::RemoveIndexServer(server_address.clone())))
            .ok_or(IndexClientError::DatabaseError)?;

        // Remove address:
        self.index_servers.retain(|cur_address| cur_address != &server_address);

        // Send report:
        let index_client_report_mutation = IndexClientReportMutation::RemoveIndexServer(server_address.clone());
        let report_mutations = vec![index_client_report_mutation];
        await!(self.to_app_server.send(IndexClientToAppServer::ReportMutations(report_mutations)))
            .map_err(|_| IndexClientError::SendToAppServerFailed)?;

        // Disconnect a current server connection if it uses the removed address:
        match &mut self.conn_status {
            ConnStatus::Empty(_) => {}, // Nothing to do here
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
            ConnStatus::Empty(_) |
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
            await!(self.seq_friends_client.mutate(mutation.clone()))
                .map_err(|_| IndexClientError::SeqFriendsError)?;
        }

        // Check if server is ready:
        let server_connected = match &mut self.conn_status {
            ConnStatus::Empty(_) |
            ConnStatus::Connecting(_) => return Ok(()), // Server is not ready
            ConnStatus::Connected(server_connected) => server_connected,
        };

        let mut control_sender = match server_connected.opt_control_sender.take() {
            Some(control_sender) => control_sender,
            None => return Ok(()),
        };

        // Append to mutations a state of a friend chosen sequentially.
        // Maybe in the future we will find a better way to do this.
        // This is important in cases where an update was not received by one of the servers.
        // Eventually this server will get an update from here:
        let next_update_res = await!(self.seq_friends_client.next_update())
            .map_err(|_| IndexClientError::SeqFriendsError)?;

        if let Some((_cycle_countdown, update_friend)) = next_update_res {
            mutations.push(IndexMutation::UpdateFriend(update_friend));
        }

        if let Ok(()) = await!(control_sender.send(SingleClientControl::SendMutations(mutations))) {
            server_connected.opt_control_sender = Some(control_sender);
        }
        // Reset ticks_to_send_keepalive:
        server_connected.ticks_to_send_keepalive = self.keepalive_ticks;

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


    pub async fn handle_index_server_connected(&mut self, control_sender: ControlSender) 
        -> Result<(), IndexClientError> {

        let address = match &self.conn_status {
            ConnStatus::Empty(_) => {
                error!("Did not attempt to connect!");
                return Ok(());
            },
            ConnStatus::Connected(_) => {
                error!("Already connected to server!");
                return Ok(());
            }
            ConnStatus::Connecting(server_connecting) => server_connecting.address.clone(),
        };

        self.conn_status = ConnStatus::Connected(ServerConnected {
            address: address.clone(),
            opt_control_sender: Some(control_sender.clone()),
            ticks_to_send_keepalive: self.keepalive_ticks,
        });

        // Send report:
        let index_client_report_mutation = IndexClientReportMutation::SetConnectedServer(Some(address.clone()));
        let report_mutations = vec![index_client_report_mutation];
        await!(self.to_app_server.send(IndexClientToAppServer::ReportMutations(report_mutations)))
            .map_err(|_| IndexClientError::SendToAppServerFailed)?;

        // Sync note: With the obtained SessionHandle (ControlSender, CloseReciever), it is
        // guaranteed that `control_sender` will stop working, and only then `close_receiver` will
        // resolve. (See IndexClientSession::connect()) This means that `send_full_state` task will
        // close before a new connection to a server is established.
        let send_full_state_fut = send_full_state(self.seq_friends_client.clone(), control_sender)
            .map_err(|e| warn!("Error in send_full_state(): {:?}", e))
            .map(|_| ());

        self.spawner.spawn(send_full_state_fut)
            .map_err(|_| IndexClientError::SpawnError)?;

        Ok(())
    }


    pub async fn handle_index_server_closed(&mut self) -> Result<(), IndexClientError> {
        self.conn_status = ConnStatus::Empty(self.backoff_ticks);

        // Send report:
        let index_client_report_mutation = IndexClientReportMutation::SetConnectedServer(None);
        let report_mutations = vec![index_client_report_mutation];
        await!(self.to_app_server.send(IndexClientToAppServer::ReportMutations(report_mutations)))
            .map_err(|_| IndexClientError::SendToAppServerFailed)
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

    pub async fn handle_timer_tick(&mut self) -> Result<(), IndexClientError> {

        // Make sure that we are connected to any server:
        let server_connected: &mut ServerConnected<ISA> = match self.conn_status {
            ConnStatus::Empty(ref mut ticks_to_reconnect) => {
                // Backoff mechanism for reconnection, so that we don't DoS the index servers.
                *ticks_to_reconnect = (*ticks_to_reconnect).saturating_sub(1);
                if *ticks_to_reconnect == 0 {
                    self.try_connect_to_server();
                }
                return Ok(());
            },
            ConnStatus::Connecting(_) => return Ok(()), // Not connected to server
            ConnStatus::Connected(ref mut server_connected) => server_connected,
        };

        let mut control_sender = match server_connected.opt_control_sender.take() {
            Some(control_sender) => control_sender,
            None => return Ok(()), // Not connected to server
        };

        server_connected.ticks_to_send_keepalive = 
            server_connected.ticks_to_send_keepalive.saturating_sub(1);

        if server_connected.ticks_to_send_keepalive != 0 {
            return Ok(());
        }

        // Reset the counter:
        server_connected.ticks_to_send_keepalive = self.keepalive_ticks;

        // We need to send a keepalive. This keepalive is in the form of a SendMutations message
        // that will be forwarded to all the servers.
        // If we don't send this periodic keepalive, index server will delete us from their
        // index.

        // Note that we might send empty mutations Vec in case we have no open friends:
        // TODO: Check the case of sending friends with (0,0) as capacity.
        let mut mutations = Vec::new();
        let next_update_res = await!(self.seq_friends_client.next_update())
            .map_err(|_| IndexClientError::SeqFriendsError)?;

        if let Some((_cycle_countdown, update_friend)) = next_update_res {
            mutations.push(IndexMutation::UpdateFriend(update_friend));
        }

        if let Ok(()) = await!(control_sender.send(SingleClientControl::SendMutations(mutations))) {
            server_connected.opt_control_sender = Some(control_sender);
        }

        Ok(())
    }
}

pub async fn index_client_loop<ISA,FAS,TAS,ICS,DB,TS,S>(from_app_server: FAS,
                               to_app_server: TAS,
                               mut index_client_config: IndexClientConfig<ISA>,
                               seq_friends_client: SeqFriendsClient,
                               index_client_session: ICS,
                               max_open_requests: usize,
                               keepalive_ticks: usize,
                               backoff_ticks: usize,
                               database: DB,
                               timer_stream: TS,
                               spawner: S) -> Result<(), IndexClientError>
where
    ISA: Eq + Clone + Send + 'static,
    FAS: Stream<Item=AppServerToIndexClient<ISA>> + Unpin,
    TAS: Sink<SinkItem=IndexClientToAppServer<ISA>> + Unpin,
    ICS: FutTransform<Input=ISA, Output=Option<SessionHandle>> + Clone + Send + 'static,
    DB: FutTransform<Input=IndexClientConfigMutation<ISA>, Output=Option<()>>,
    TS: Stream + Unpin,
    S: Spawn,
{
    let (event_sender, event_receiver) = mpsc::channel(0);
    let mut index_client = IndexClient::new(event_sender, 
                                            to_app_server,
                                            index_client_config,
                                            seq_friends_client,
                                            index_client_session,
                                            max_open_requests, 
                                            keepalive_ticks,
                                            backoff_ticks,
                                            database,
                                            spawner);
    
    index_client.try_connect_to_server()?;

    let timer_stream = timer_stream
        .map(|_| IndexClientEvent::TimerTick);

    let from_app_server = from_app_server
        .map(|app_server_to_index_client| IndexClientEvent::FromAppServer(app_server_to_index_client))
        .chain(stream::once(future::ready(IndexClientEvent::AppServerClosed)));

    let mut events = event_receiver
        .select(from_app_server)
        .select(timer_stream);

    while let Some(event) = await!(events.next()) {
        match event {
            IndexClientEvent::FromAppServer(app_server_to_index_client) =>
                await!(index_client.handle_from_app_server(app_server_to_index_client))?,
            IndexClientEvent::AppServerClosed => return Err(IndexClientError::AppServerClosed),
            IndexClientEvent::IndexServerConnected(control_sender) => 
                await!(index_client.handle_index_server_connected(control_sender))?,
            IndexClientEvent::IndexServerClosed =>
                await!(index_client.handle_index_server_closed())?,
            IndexClientEvent::ResponseRoutes((request_id, response_routes_result)) =>
                await!(index_client.handle_response_routes(request_id, response_routes_result))?,
            IndexClientEvent::TimerTick =>
                await!(index_client.handle_timer_tick())?,
        };
    }
    Ok(())
}
