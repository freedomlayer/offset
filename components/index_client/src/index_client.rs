use std::collections::VecDeque;
use std::fmt::Debug;
use std::marker::Unpin;

use futures::channel::{mpsc, oneshot};
use futures::task::{Spawn, SpawnExt};
use futures::{future, select, stream, FutureExt, Sink, SinkExt, Stream, StreamExt, TryFutureExt};

use common::conn::{BoxStream, FutTransform};
use common::mutable_state::MutableState;
use common::never::Never;
use common::select_streams::select_streams;

use proto::crypto::{PublicKey, Uid};

use database::DatabaseClient;

use proto::index_client::messages::{
    AppServerToIndexClient, ClientResponseRoutes, IndexClientReportMutation,
    IndexClientReportMutations, IndexClientRequest, IndexClientToAppServer, IndexMutation,
    RequestRoutes, ResponseRoutesResult,
};
use proto::index_server::messages::{IndexServerAddress, NamedIndexServerAddress};

use crate::client_session::{ControlSender, SessionHandle};
use crate::seq_friends::SeqFriendsClient;
use crate::single_client::SingleClientControl;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct IndexClientConfig<ISA> {
    pub index_servers: Vec<NamedIndexServerAddress<ISA>>,
}

impl<ISA> IndexClientConfig<ISA> {
    pub fn new() -> Self {
        IndexClientConfig {
            index_servers: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum IndexClientConfigMutation<ISA> {
    AddIndexServer(NamedIndexServerAddress<ISA>),
    RemoveIndexServer(PublicKey),
}

impl<ISA> MutableState for IndexClientConfig<ISA>
where
    ISA: Clone + PartialEq + Eq,
{
    type Mutation = IndexClientConfigMutation<ISA>;
    type MutateError = Never;

    fn mutate(&mut self, mutation: &Self::Mutation) -> Result<(), Self::MutateError> {
        match mutation {
            IndexClientConfigMutation::AddIndexServer(named_index_server_address) => {
                // Remove first, to avoid duplicates:
                self.index_servers.retain(|cur_named_index_server_address| {
                    cur_named_index_server_address.public_key
                        != named_index_server_address.public_key
                });
                self.index_servers.push(named_index_server_address.clone());
            }
            IndexClientConfigMutation::RemoveIndexServer(public_key) => {
                self.index_servers
                    .retain(|named_index_server| &named_index_server.public_key != public_key);
            }
        };
        Ok(())
    }
}

#[derive(Debug)]
struct ServerConnecting<ISA> {
    index_server: IndexServerAddress<ISA>,
    opt_cancel_sender: Option<oneshot::Sender<()>>,
}

#[derive(Debug)]
struct ServerConnected<ISA> {
    index_server: IndexServerAddress<ISA>,
    opt_control_sender: Option<ControlSender>,
    /// A oneshot for closing the connection
    /// (closing opt_control_sender is not enough, because send_full_state() task also has a
    /// sender)
    opt_cancel_sender: Option<oneshot::Sender<()>>,
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

struct IndexClient<ISA, TAS, ICS, S> {
    event_sender: mpsc::Sender<IndexClientEvent<ISA>>,
    to_app_server: TAS,
    /// A cyclic list of index server addresses.
    /// The next index server to be used is the one on the front:
    // TODO: Why not use IndexClientConfig as state here?
    // We perform the mutations implicitly in the implementation of IndexClient.
    index_servers: VecDeque<IndexServerAddress<ISA>>,
    seq_friends_client: SeqFriendsClient,
    index_client_session: ICS,
    max_open_requests: usize,
    num_open_requests: usize,
    keepalive_ticks: usize,
    backoff_ticks: usize,
    conn_status: ConnStatus<ISA>,
    db_client: DatabaseClient<IndexClientConfigMutation<ISA>>,
    spawner: S,
}

/// Send our full friends state as mutations to the server.
/// We do this in a separate task so that we don't block user requests or incoming funder reports.
async fn send_full_state(
    mut seq_friends_client: SeqFriendsClient,
    mut control_sender: ControlSender,
) -> Result<(), IndexClientError> {
    seq_friends_client
        .reset_countdown()
        .await
        .map_err(|_| IndexClientError::SeqFriendsError)?;

    loop {
        let next_update_res = seq_friends_client
            .next_update()
            .await
            .map_err(|_| IndexClientError::SeqFriendsError)?;

        let (cyclic_countdown, update_friend_currency) = match next_update_res {
            Some(next_update) => next_update,
            None => break,
        };

        // TODO: Maybe send mutations in batches in the future:
        // However, we need to be careful to not send too many mutations in one batch.
        let mutations = vec![IndexMutation::UpdateFriendCurrency(update_friend_currency)];
        if control_sender
            .send(SingleClientControl::SendMutations(mutations))
            .await
            .is_err()
        {
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

impl<ISA, TAS, ICS, S> IndexClient<ISA, TAS, ICS, S>
where
    ISA: Debug + Eq + Clone + Send + 'static,
    TAS: Sink<IndexClientToAppServer<ISA>> + Unpin,
    ICS: FutTransform<Input = IndexServerAddress<ISA>, Output = Option<SessionHandle>>
        + Clone
        + Send
        + 'static,
    S: Spawn + Clone + Send + 'static,
{
    pub fn new(
        event_sender: mpsc::Sender<IndexClientEvent<ISA>>,
        to_app_server: TAS,
        index_client_config: IndexClientConfig<ISA>,
        seq_friends_client: SeqFriendsClient,
        index_client_session: ICS,
        max_open_requests: usize,
        keepalive_ticks: usize,
        backoff_ticks: usize,
        db_client: DatabaseClient<IndexClientConfigMutation<ISA>>,
        spawner: S,
    ) -> Self {
        let index_servers = index_client_config
            .index_servers
            .into_iter()
            .map(|named_index_server| IndexServerAddress {
                public_key: named_index_server.public_key,
                address: named_index_server.address,
            })
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
            db_client,
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

        let index_server = match self.index_servers.pop_front() {
            Some(index_server) => index_server,
            None => {
                // We don't have any index servers to connect to:
                self.conn_status = ConnStatus::Empty(0);
                return Ok(());
            }
        };
        // Move the chosen address to the end, rotating the addresses VecDeque 1 to the left:
        self.index_servers.push_back(index_server.clone());

        let mut c_index_client_session = self.index_client_session.clone();
        let mut c_event_sender = self.event_sender.clone();

        let (cancel_sender, cancel_receiver) = oneshot::channel();
        let server_connecting = ServerConnecting {
            index_server: index_server.clone(),
            opt_cancel_sender: Some(cancel_sender),
        };
        self.conn_status = ConnStatus::Connecting(server_connecting);

        let c_seq_friends_client = self.seq_friends_client.clone();
        let c_spawner = self.spawner.clone();

        // Canceller for the send_full_state() task:
        let (sfs_cancel_sender, sfs_cancel_receiver) = oneshot::channel::<()>();
        let (sfs_done_sender, sfs_done_receiver) = oneshot::channel::<()>();

        // TODO: Can we remove the Box::pin() from here? How?
        let connect_fut = Box::pin(async move {
            let res = c_index_client_session.transform(index_server).await?;
            let (control_sender, close_receiver) = res;

            let c_control_sender = control_sender.clone();
            let send_full_state_cancellable_fut = async move {
                let send_full_state_fut = Box::pin(
                    send_full_state(c_seq_friends_client, c_control_sender)
                        .map_err(|e| warn!("Error in send_full_state(): {:?}", e))
                        .map(|_| {
                            let _ = sfs_done_sender.send(());
                        }),
                );

                select! {
                    _sfs_cancel_receiver = sfs_cancel_receiver.fuse() => (),
                    _ = send_full_state_fut.fuse() => (),
                };
            };

            c_spawner.spawn(send_full_state_cancellable_fut).ok()?;

            let _ = c_event_sender
                .send(IndexClientEvent::IndexServerConnected(control_sender))
                .await;
            let _ = close_receiver.await;
            Some(())
        });

        let mut c_event_sender = self.event_sender.clone();
        let cancellable_fut = async move {
            // During the connection stage it is possible to cancel using the `cancel_sender`:
            select! {
                _connect_fut = connect_fut.fuse() => (),
                _ = cancel_receiver.fuse() => (),
            };
            // Connection was closed or cancelled:

            // Cancel send_full_state() task:
            let _ = sfs_cancel_sender.send(());
            // Wait until send_full_state() task is done:
            let _ = sfs_done_receiver.await;

            // Notify main task about closed connection:
            let _ = c_event_sender
                .send(IndexClientEvent::IndexServerClosed)
                .await;
        };

        self.spawner
            .spawn(cancellable_fut)
            .map_err(|_| IndexClientError::SpawnError)
    }

    pub async fn return_response_routes_failure(
        &mut self,
        request_id: Uid,
    ) -> Result<(), IndexClientError> {
        let client_response_routes = ClientResponseRoutes {
            request_id,
            result: ResponseRoutesResult::Failure,
        };
        self.to_app_server
            .send(IndexClientToAppServer::ResponseRoutes(
                client_response_routes,
            ))
            .await
            .map_err(|_| IndexClientError::SendToAppServerFailed)
    }

    pub async fn handle_from_app_server_add_index_server(
        &mut self,
        app_request_id: Uid,
        named_index_server_address: NamedIndexServerAddress<ISA>,
    ) -> Result<(), IndexClientError> {
        // Update database:
        self.db_client
            .mutate(vec![IndexClientConfigMutation::AddIndexServer(
                named_index_server_address.clone(),
            )])
            .await
            .map_err(|_| IndexClientError::DatabaseError)?;

        // Add new server_address to memory:
        // To avoid duplicates, we try to remove it from the list first:
        self.index_servers.retain(|index_server| {
            index_server.public_key != named_index_server_address.public_key
        });

        let index_server = IndexServerAddress::from(named_index_server_address.clone());
        self.index_servers.push_back(index_server);

        // Send report to AppServer:
        let index_client_report_mutation =
            IndexClientReportMutation::AddIndexServer(named_index_server_address.clone());
        let index_client_report_mutations = IndexClientReportMutations {
            opt_app_request_id: Some(app_request_id),
            mutations: vec![index_client_report_mutation],
        };
        self.to_app_server
            .send(IndexClientToAppServer::ReportMutations(
                index_client_report_mutations,
            ))
            .await
            .map_err(|_| IndexClientError::SendToAppServerFailed)?;

        if let ConnStatus::Empty(_) = self.conn_status {
        } else {
            return Ok(());
        }
        // We are here if conn_status is empty.
        // This means we should initiate connection to an index server
        self.try_connect_to_server()
    }

    pub async fn handle_from_app_server_remove_index_server(
        &mut self,
        app_request_id: Uid,
        public_key: PublicKey,
    ) -> Result<(), IndexClientError> {
        // Update database:
        self.db_client
            .mutate(vec![IndexClientConfigMutation::RemoveIndexServer(
                public_key.clone(),
            )])
            .await
            .map_err(|_| IndexClientError::DatabaseError)?;

        // Remove address:
        self.index_servers
            .retain(|index_server| index_server.public_key != public_key);

        // Send report:
        let index_client_report_mutation =
            IndexClientReportMutation::RemoveIndexServer(public_key.clone());
        let index_client_report_mutations = IndexClientReportMutations {
            opt_app_request_id: Some(app_request_id),
            mutations: vec![index_client_report_mutation],
        };
        self.to_app_server
            .send(IndexClientToAppServer::ReportMutations(
                index_client_report_mutations,
            ))
            .await
            .map_err(|_| IndexClientError::SendToAppServerFailed)?;

        // Disconnect a current server connection if it uses the removed address:
        match &mut self.conn_status {
            ConnStatus::Empty(_) => {} // Nothing to do here
            ConnStatus::Connecting(server_connecting) => {
                if server_connecting.index_server.public_key == public_key {
                    if let Some(cancel_sender) = server_connecting.opt_cancel_sender.take() {
                        let _ = cancel_sender.send(());
                    }
                }
            }
            ConnStatus::Connected(server_connected) => {
                if server_connected.index_server.public_key == public_key {
                    server_connected.opt_control_sender.take();
                    server_connected.opt_cancel_sender.take();
                }
            }
        }
        Ok(())
    }

    pub async fn handle_from_app_server_request_routes(
        &mut self,
        app_request_id: Uid,
        request_routes: RequestRoutes,
    ) -> Result<(), IndexClientError> {
        // Send empty report (Indicates that we received the request):
        let index_client_report_mutations = IndexClientReportMutations {
            opt_app_request_id: Some(app_request_id),
            mutations: Vec::new(),
        };
        self.to_app_server
            .send(IndexClientToAppServer::ReportMutations(
                index_client_report_mutations,
            ))
            .await
            .map_err(|_| IndexClientError::SendToAppServerFailed)?;

        if self.num_open_requests >= self.max_open_requests {
            return self
                .return_response_routes_failure(request_routes.request_id)
                .await;
        }

        // Check server connection status:
        let mut server_connected = match &mut self.conn_status {
            ConnStatus::Empty(_) | ConnStatus::Connecting(_) => {
                return self
                    .return_response_routes_failure(request_routes.request_id)
                    .await
            }
            ConnStatus::Connected(server_connected) => server_connected,
        };

        let mut control_sender = match server_connected.opt_control_sender.take() {
            Some(control_sender) => control_sender,
            None => {
                return self
                    .return_response_routes_failure(request_routes.request_id)
                    .await
            }
        };

        let c_request_id = request_routes.request_id.clone();
        let (response_sender, response_receiver) = oneshot::channel();
        let single_client_control =
            SingleClientControl::RequestRoutes((request_routes, response_sender));

        match control_sender.send(single_client_control).await {
            Ok(()) => server_connected.opt_control_sender = Some(control_sender),
            Err(_) => return self.return_response_routes_failure(c_request_id).await,
        };

        let mut c_event_sender = self.event_sender.clone();
        let request_fut = async move {
            let response_routes_result = match response_receiver.await {
                Ok(routes) => ResponseRoutesResult::Success(routes),
                Err(_) => ResponseRoutesResult::Failure,
            };
            // TODO: Should report error here if failure occurs?
            let _ = c_event_sender
                .send(IndexClientEvent::ResponseRoutes((
                    c_request_id,
                    response_routes_result,
                )))
                .await;
        };

        self.num_open_requests = self.num_open_requests.saturating_add(1);
        self.spawner
            .spawn(request_fut)
            .map_err(|_| IndexClientError::SpawnError)
    }

    pub async fn handle_from_app_server_apply_mutations(
        &mut self,
        mut mutations: Vec<IndexMutation>,
    ) -> Result<(), IndexClientError> {
        // Update state:
        for mutation in &mutations {
            self.seq_friends_client
                .mutate(mutation.clone())
                .await
                .map_err(|_| IndexClientError::SeqFriendsError)?;
        }

        // Check if server is ready:
        let server_connected = match &mut self.conn_status {
            ConnStatus::Empty(_) | ConnStatus::Connecting(_) => return Ok(()), // Server is not ready
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
        let next_update_res = self
            .seq_friends_client
            .next_update()
            .await
            .map_err(|_| IndexClientError::SeqFriendsError)?;

        if let Some((_cycle_countdown, update_friend_currency)) = next_update_res {
            mutations.push(IndexMutation::UpdateFriendCurrency(update_friend_currency));
        }

        if let Ok(()) = control_sender
            .send(SingleClientControl::SendMutations(mutations))
            .await
        {
            server_connected.opt_control_sender = Some(control_sender);
        }
        // Reset ticks_to_send_keepalive:
        server_connected.ticks_to_send_keepalive = self.keepalive_ticks;

        Ok(())
    }

    pub async fn handle_from_app_server(
        &mut self,
        app_server_to_index_client: AppServerToIndexClient<ISA>,
    ) -> Result<(), IndexClientError> {
        match app_server_to_index_client {
            AppServerToIndexClient::AppRequest((app_request_id, index_client_request)) => {
                match index_client_request {
                    IndexClientRequest::AddIndexServer(add_index_server) => {
                        self.handle_from_app_server_add_index_server(
                            app_request_id,
                            add_index_server,
                        )
                        .await
                    }
                    IndexClientRequest::RemoveIndexServer(public_key) => {
                        self.handle_from_app_server_remove_index_server(app_request_id, public_key)
                            .await
                    }
                    IndexClientRequest::RequestRoutes(request_routes) => {
                        self.handle_from_app_server_request_routes(app_request_id, request_routes)
                            .await
                    }
                }
            }
            AppServerToIndexClient::ApplyMutations(mutations) => {
                self.handle_from_app_server_apply_mutations(mutations).await
            }
        }
    }

    pub async fn handle_index_server_connected(
        &mut self,
        control_sender: ControlSender,
    ) -> Result<(), IndexClientError> {
        let (index_server, opt_cancel_sender) = match &mut self.conn_status {
            ConnStatus::Empty(_) => {
                error!("Did not attempt to connect!");
                return Ok(());
            }
            ConnStatus::Connected(_) => {
                error!("Already connected to server!");
                return Ok(());
            }
            ConnStatus::Connecting(server_connecting) => (
                server_connecting.index_server.clone(),
                server_connecting.opt_cancel_sender.take(),
            ),
        };

        self.conn_status = ConnStatus::Connected(ServerConnected {
            index_server: index_server.clone(),
            opt_control_sender: Some(control_sender.clone()),
            opt_cancel_sender,
            ticks_to_send_keepalive: self.keepalive_ticks,
        });

        // Send report:
        let index_client_report_mutation =
            IndexClientReportMutation::SetConnectedServer(Some(index_server.public_key.clone()));
        let index_client_report_mutations = IndexClientReportMutations {
            opt_app_request_id: None,
            mutations: vec![index_client_report_mutation],
        };
        self.to_app_server
            .send(IndexClientToAppServer::ReportMutations(
                index_client_report_mutations,
            ))
            .await
            .map_err(|_| IndexClientError::SendToAppServerFailed)?;

        Ok(())
    }

    pub async fn handle_index_server_closed(&mut self) -> Result<(), IndexClientError> {
        if let ConnStatus::Connected(_) = self.conn_status {
            // Send report:
            let index_client_report_mutation = IndexClientReportMutation::SetConnectedServer(None);
            let index_client_report_mutations = IndexClientReportMutations {
                opt_app_request_id: None,
                mutations: vec![index_client_report_mutation],
            };
            self.to_app_server
                .send(IndexClientToAppServer::ReportMutations(
                    index_client_report_mutations,
                ))
                .await
                .map_err(|_| IndexClientError::SendToAppServerFailed)?;
        }
        self.conn_status = ConnStatus::Empty(self.backoff_ticks);
        Ok(())
    }

    pub async fn handle_response_routes(
        &mut self,
        request_id: Uid,
        response_routes_result: ResponseRoutesResult,
    ) -> Result<(), IndexClientError> {
        self.num_open_requests = self.num_open_requests.checked_sub(1).unwrap();

        let client_response_routes = ClientResponseRoutes {
            request_id,
            result: response_routes_result,
        };

        self.to_app_server
            .send(IndexClientToAppServer::ResponseRoutes(
                client_response_routes,
            ))
            .await
            .map_err(|_| IndexClientError::SendToAppServerFailed)
    }

    pub async fn handle_timer_tick(&mut self) -> Result<(), IndexClientError> {
        // Make sure that we are connected to any server:
        let server_connected: &mut ServerConnected<ISA> = match self.conn_status {
            ConnStatus::Empty(ref mut ticks_to_reconnect) => {
                // Backoff mechanism for reconnection, so that we don't DoS the index servers.
                *ticks_to_reconnect = (*ticks_to_reconnect).saturating_sub(1);
                if *ticks_to_reconnect == 0 {
                    self.try_connect_to_server()?;
                }
                return Ok(());
            }
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
            server_connected.opt_control_sender = Some(control_sender);
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
        let next_update_res = self
            .seq_friends_client
            .next_update()
            .await
            .map_err(|_| IndexClientError::SeqFriendsError)?;

        if let Some((_cycle_countdown, update_friend_currency)) = next_update_res {
            mutations.push(IndexMutation::UpdateFriendCurrency(update_friend_currency));
        }

        if let Ok(()) = control_sender
            .send(SingleClientControl::SendMutations(mutations))
            .await
        {
            server_connected.opt_control_sender = Some(control_sender);
        }

        Ok(())
    }
}

pub async fn index_client_loop<ISA, FAS, TAS, ICS, TS, S>(
    from_app_server: FAS,
    to_app_server: TAS,
    index_client_config: IndexClientConfig<ISA>,
    seq_friends_client: SeqFriendsClient,
    index_client_session: ICS,
    max_open_requests: usize,
    keepalive_ticks: usize,
    backoff_ticks: usize,
    db_client: DatabaseClient<IndexClientConfigMutation<ISA>>,
    timer_stream: TS,
    spawner: S,
) -> Result<(), IndexClientError>
where
    ISA: Debug + Eq + Clone + Send + 'static,
    FAS: Stream<Item = AppServerToIndexClient<ISA>> + Send + Unpin,
    TAS: Sink<IndexClientToAppServer<ISA>> + Unpin,
    ICS: FutTransform<Input = IndexServerAddress<ISA>, Output = Option<SessionHandle>>
        + Clone
        + Send
        + 'static,
    TS: Stream + Send + Unpin,
    S: Spawn + Clone + Send + 'static,
{
    let (event_sender, event_receiver) = mpsc::channel(0);
    let mut index_client = IndexClient::new(
        event_sender,
        to_app_server,
        index_client_config,
        seq_friends_client,
        index_client_session,
        max_open_requests,
        keepalive_ticks,
        backoff_ticks,
        db_client,
        spawner,
    );

    index_client.try_connect_to_server()?;

    let timer_stream = timer_stream.map(|_| IndexClientEvent::TimerTick);

    let from_app_server = from_app_server
        .map(|app_server_to_index_client| {
            IndexClientEvent::FromAppServer(app_server_to_index_client)
        })
        .chain(stream::once(future::ready(
            IndexClientEvent::AppServerClosed,
        )));

    let mut events = select_streams![event_receiver, from_app_server, timer_stream];

    while let Some(event) = events.next().await {
        match event {
            IndexClientEvent::FromAppServer(app_server_to_index_client) => {
                index_client
                    .handle_from_app_server(app_server_to_index_client)
                    .await?
            }
            IndexClientEvent::AppServerClosed => return Err(IndexClientError::AppServerClosed),
            IndexClientEvent::IndexServerConnected(control_sender) => {
                index_client
                    .handle_index_server_connected(control_sender)
                    .await?
            }
            IndexClientEvent::IndexServerClosed => {
                index_client.handle_index_server_closed().await?
            }
            IndexClientEvent::ResponseRoutes((request_id, response_routes_result)) => {
                index_client
                    .handle_response_routes(request_id, response_routes_result)
                    .await?
            }
            IndexClientEvent::TimerTick => index_client.handle_timer_tick().await?,
        };
    }
    Ok(())
}
