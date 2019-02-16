use std::marker::Unpin;
use std::fmt::Debug;
use std::collections::{HashMap, HashSet};

use futures::{future, stream, Stream, StreamExt, Sink, SinkExt};
use futures::channel::mpsc;
use futures::task::{Spawn, SpawnExt};

use common::conn::ConnPair;
use common::mutable_state::MutableState;
use crypto::uid::Uid;

use proto::funder::messages::{FunderOutgoingControl, FunderIncomingControl, 
    RemoveFriend, SetFriendStatus, FriendStatus,
    RequestsStatus, SetRequestsStatus};
use proto::report::messages::funder_report_mutation_to_index_mutation;

use proto::app_server::messages::{AppServerToApp, AppToAppServer, NodeReport,
                                    NodeReportMutation, AppPermissions};
use proto::index_client::messages::{IndexClientToAppServer, AppServerToIndexClient,
                                    AddIndexServer};

pub type IncomingAppConnection<B> = (AppPermissions, 
                                     ConnPair<AppServerToApp<B>, AppToAppServer<B>>);


#[derive(Debug)]
pub enum AppServerError {
    FunderClosed,
    SpawnError,
    IndexClientClosed,
    SendToFunderError,
    SendToIndexClientError,
    AllAppsClosed,
}

pub enum AppServerEvent<B:Clone> {
    IncomingConnection(IncomingAppConnection<B>),
    IncomingConnectionsClosed,
    FromFunder(FunderOutgoingControl<B>),
    FunderClosed,
    FromIndexClient(IndexClientToAppServer<B>),
    IndexClientClosed,
    FromApp((u128, Option<AppToAppServer<B>>)), // None means that app was closed
}

pub struct App<B:Clone> {
    permissions: AppPermissions,
    opt_sender: Option<mpsc::Sender<AppServerToApp<B>>>,
    open_route_requests: HashSet<Uid>,
    open_send_funds_requests: HashSet<Uid>,
}

impl<B> App<B> 
where
    B: Clone,
{
    pub fn new(permissions: AppPermissions,
               sender: mpsc::Sender<AppServerToApp<B>>) -> Self {

        App {
            permissions,
            opt_sender: Some(sender),
            open_route_requests: HashSet::new(),
            open_send_funds_requests: HashSet::new(),
        }
    }

    pub async fn send(&mut self, message: AppServerToApp<B>)  {
        match self.opt_sender.take() {
            Some(mut sender) => {
                if let Ok(()) = await!(sender.send(message)) {
                    self.opt_sender = Some(sender);
                }
            },
            None => {},
        }
    }
}


pub struct AppServer<B:Clone,TF,TIC,S> {
    to_funder: TF,
    to_index_client: TIC,
    from_app_sender: mpsc::Sender<(u128, Option<AppToAppServer<B>>)>,
    node_report: NodeReport<B>,
    incoming_connections_closed: bool,
    /// A long cyclic incrementing counter, 
    /// allows to give every connection a unique number.
    /// Required because an app (with one public key) might have multiple connections.
    app_counter: u128,
    apps: HashMap<u128, App<B>>,
    spawner: S,
}

/// Check if we should process an app_message from an app with certain permissions
fn check_permissions<B>(app_permissions: &AppPermissions, 
                     app_message: &AppToAppServer<B>) -> bool {

    match app_message {
        AppToAppServer::AddRelay(_) => app_permissions.config,
        AppToAppServer::RemoveRelay(_) => app_permissions.config,
        AppToAppServer::RequestSendFunds(_) => app_permissions.send_funds,
        AppToAppServer::ReceiptAck(_) => app_permissions.send_funds,
        AppToAppServer::AddFriend(_) => app_permissions.config,
        AppToAppServer::SetFriendRelays(_) => app_permissions.config,
        AppToAppServer::SetFriendName(_) => app_permissions.config,
        AppToAppServer::RemoveFriend(_) => app_permissions.config,
        AppToAppServer::EnableFriend(_) => app_permissions.config,
        AppToAppServer::DisableFriend(_) => app_permissions.config,
        AppToAppServer::OpenFriend(_) => app_permissions.config,
        AppToAppServer::CloseFriend(_) => app_permissions.config,
        AppToAppServer::SetFriendRemoteMaxDebt(_) => app_permissions.config,
        AppToAppServer::ResetFriendChannel(_) => app_permissions.config,
        AppToAppServer::RequestRoutes(_) => app_permissions.routes,
        AppToAppServer::AddIndexServer(_) => app_permissions.config,
        AppToAppServer::RemoveIndexServer(_) => app_permissions.config,
    }
}

impl<B,TF,TIC,S> AppServer<B,TF,TIC,S> 
where
    B: Clone + PartialEq + Eq + Debug + Send + Sync + 'static,
    TF: Sink<SinkItem=FunderIncomingControl<B>> + Unpin + Sync + Send,
    TIC: Sink<SinkItem=AppServerToIndexClient<B>> + Unpin,
    S: Spawn,
{
    pub fn new(to_funder: TF, 
               to_index_client: TIC,
               from_app_sender: mpsc::Sender<(u128, Option<AppToAppServer<B>>)>,
               node_report: NodeReport<B>,
               spawner: S) -> Self {

        AppServer {
            to_funder,
            to_index_client,
            from_app_sender,
            node_report,
            incoming_connections_closed: false,
            app_counter: 0,
            apps: HashMap::new(),
            spawner,
        }
    }

    /// Add an application connection
    pub async fn handle_incoming_connection(&mut self, incoming_app_connection: IncomingAppConnection<B>) 
        -> Result<(), AppServerError> {

        let (permissions, (sender, receiver)) = incoming_app_connection;

        let app_counter = self.app_counter;
        let mut receiver = receiver
            .map(move |app_to_app_server| (app_counter.clone(), Some(app_to_app_server)));

        let mut from_app_sender = self.from_app_sender.clone();
        let send_all_fut = async move {
            // Forward all messages:
            let _ = await!(from_app_sender.send_all(&mut receiver));
            // Notify that the connection to the app was closed:
            let _ = await!(from_app_sender.send((app_counter, None)));
        };

        self.spawner.spawn(send_all_fut)
            .map_err(|_| AppServerError::SpawnError)?;

        let mut app = App::new(permissions, sender);
        // Possibly send the initial node report:
        if app.permissions.reports {
            await!(app.send(AppServerToApp::Report(self.node_report.clone())));
        }

        self.apps.insert(self.app_counter, app);
        self.app_counter = self.app_counter.wrapping_add(1);

        Ok(())

    }

    /// The channel carrying new connections was closed. 
    /// This means we will not receive any new connections
    pub async fn handle_incoming_connections_closed(&mut self) -> Result<(), AppServerError> {
        self.incoming_connections_closed = true;
        if self.apps.is_empty() {
            return Err(AppServerError::AllAppsClosed);
        }
        Ok(())
    }

    /// Send node report mutations to all connected apps
    pub async fn broadcast_node_report_mutations(&mut self, 
                                                 node_report_mutations: Vec<NodeReportMutation<B>>) {

        // Send node report mutations to all connected apps
        for (_app_id, app) in &mut self.apps {
            if app.permissions.reports {
                await!(app.send(AppServerToApp::ReportMutations(node_report_mutations.clone())));
            }
        }
    }

    pub async fn handle_from_funder(&mut self, funder_message: FunderOutgoingControl<B>)
        -> Result<(), AppServerError> {

        match funder_message {
            FunderOutgoingControl::ResponseReceived(response_received) => {
                // Find the app that issued the request, and forward the response to this app:
                // TODO: Should we break the loop if found?
                for (_app_id, app) in &mut self.apps {
                    if app.open_send_funds_requests.remove(&response_received.request_id) {
                        await!(app.send(AppServerToApp::ResponseReceived(response_received.clone())));
                    }
                }
            },
            FunderOutgoingControl::ReportMutations(report_mutations) => {
                let mut index_mutations = Vec::new();
                for funder_report_mutation in &report_mutations {
                    // Transform the funder report mutation to index mutations
                    // and send it to IndexClient
                    let opt_index_mutation = funder_report_mutation_to_index_mutation(
                            &self.node_report.funder_report,
                            funder_report_mutation);

                    if let Some(index_mutation) = opt_index_mutation {
                        index_mutations.push(index_mutation);
                    }
                }

                // Send index mutations:
                if !index_mutations.is_empty() {
                    await!(self.to_index_client.send(AppServerToIndexClient::ApplyMutations(index_mutations)))
                        .map_err(|_| AppServerError::SendToIndexClientError)?;
                }

                let mut node_report_mutations = Vec::new();
                for funder_report_mutation in report_mutations {
                    let mutation = NodeReportMutation::Funder(funder_report_mutation);
                    // Mutate our node report:
                    self.node_report.mutate(&mutation).unwrap();
                    node_report_mutations.push(mutation);
                }

                await!(self.broadcast_node_report_mutations(node_report_mutations));

            },
        }
        Ok(())
    }

    pub async fn handle_from_index_client(&mut self, index_client_message: IndexClientToAppServer<B>) 
        -> Result<(), AppServerError> {

        match index_client_message {
            IndexClientToAppServer::ReportMutations(report_mutations) => {
                let mut node_report_mutations = Vec::new();
                for index_client_report_mutation in report_mutations {
                    let mutation = NodeReportMutation::IndexClient(index_client_report_mutation);
                    // Mutate our node report:
                    self.node_report.mutate(&mutation).unwrap();
                    node_report_mutations.push(mutation);
                }

                await!(self.broadcast_node_report_mutations(node_report_mutations));
            },
            IndexClientToAppServer::ResponseRoutes(client_response_routes) => {
                // We search for the app that issued the request, and send it the response.
                // TODO: Should we break the loop if we found one originating app?
                for (_app_id, app) in &mut self.apps {
                    if app.open_route_requests.remove(&client_response_routes.request_id) {
                        await!(app.send(AppServerToApp::ResponseRoutes(client_response_routes.clone())));
                    }
                }
            },
        };
        Ok(())
    }

    async fn handle_app_message(&mut self, app_id: u128, app_message: AppToAppServer<B>)
        -> Result<(), AppServerError> {

        // Get the relevant application:
        let app = match self.apps.get_mut(&app_id) {
            Some(app) => app,
            None => {
                warn!("App {:?} does not exist!", app_id);
                return Ok(());
            },
        };

        // Make sure this message is allowed for this application:
        if !check_permissions(&app.permissions, &app_message) {
            warn!("App {:?} does not have permissions for {:?}", app_id, app_message);
            return Ok(());
        }

        match app_message {
            AppToAppServer::AddRelay(named_relay_address) =>
                await!(self.to_funder.send(FunderIncomingControl::AddRelay(named_relay_address)))
                    .map_err(|_| AppServerError::SendToFunderError),
            AppToAppServer::RemoveRelay(public_key) =>
                await!(self.to_funder.send(FunderIncomingControl::RemoveRelay(public_key)))
                    .map_err(|_| AppServerError::SendToFunderError),
            AppToAppServer::RequestSendFunds(user_request_send_funds) => {
                // Keep track of which application issued this request:
                app.open_send_funds_requests.insert(user_request_send_funds.request_id.clone());
                await!(self.to_funder.send(FunderIncomingControl::RequestSendFunds(user_request_send_funds)))
                    .map_err(|_| AppServerError::SendToFunderError)
            },
            AppToAppServer::ReceiptAck(receipt_ack) =>
                await!(self.to_funder.send(FunderIncomingControl::ReceiptAck(receipt_ack)))
                    .map_err(|_| AppServerError::SendToFunderError),
            AppToAppServer::AddFriend(add_friend) =>
                await!(self.to_funder.send(FunderIncomingControl::AddFriend(add_friend)))
                    .map_err(|_| AppServerError::SendToFunderError),
            AppToAppServer::SetFriendRelays(set_friend_address) =>
                await!(self.to_funder.send(FunderIncomingControl::SetFriendRelays(set_friend_address)))
                    .map_err(|_| AppServerError::SendToFunderError),
            AppToAppServer::SetFriendName(set_friend_name) => 
                await!(self.to_funder.send(FunderIncomingControl::SetFriendName(set_friend_name)))
                    .map_err(|_| AppServerError::SendToFunderError),
            AppToAppServer::RemoveFriend(friend_public_key) => {
                let remove_friend = RemoveFriend { friend_public_key };
                await!(self.to_funder.send(FunderIncomingControl::RemoveFriend(remove_friend)))
                    .map_err(|_| AppServerError::SendToFunderError)
            },
            AppToAppServer::EnableFriend(friend_public_key) => {
                let set_friend_status = SetFriendStatus {
                    friend_public_key,
                    status: FriendStatus::Enabled,
                };
                await!(self.to_funder.send(FunderIncomingControl::SetFriendStatus(set_friend_status)))
                    .map_err(|_| AppServerError::SendToFunderError)
            },
            AppToAppServer::DisableFriend(friend_public_key) => {
                let set_friend_status = SetFriendStatus {
                    friend_public_key,
                    status: FriendStatus::Disabled,
                };
                await!(self.to_funder.send(FunderIncomingControl::SetFriendStatus(set_friend_status)))
                    .map_err(|_| AppServerError::SendToFunderError)
            },
            AppToAppServer::OpenFriend(friend_public_key) => {
                let set_requests_status = SetRequestsStatus {
                    friend_public_key,
                    status: RequestsStatus::Open,
                };
                await!(self.to_funder.send(FunderIncomingControl::SetRequestsStatus(set_requests_status))) 
                    .map_err(|_| AppServerError::SendToFunderError)
            },
            AppToAppServer::CloseFriend(friend_public_key) => {
                let set_requests_status = SetRequestsStatus {
                    friend_public_key,
                    status: RequestsStatus::Closed,
                };
                await!(self.to_funder.send(FunderIncomingControl::SetRequestsStatus(set_requests_status)))
                    .map_err(|_| AppServerError::SendToFunderError)
            },
            AppToAppServer::SetFriendRemoteMaxDebt(set_friend_remote_max_debt) =>
                await!(self.to_funder.send(FunderIncomingControl::SetFriendRemoteMaxDebt(set_friend_remote_max_debt)))
                    .map_err(|_| AppServerError::SendToFunderError),
            AppToAppServer::ResetFriendChannel(reset_friend_channel) =>
                await!(self.to_funder.send(FunderIncomingControl::ResetFriendChannel(reset_friend_channel)))
                    .map_err(|_| AppServerError::SendToFunderError),
            AppToAppServer::RequestRoutes(request_routes) => {
                // Keep track of which application issued this request:
                app.open_route_requests.insert(request_routes.request_id.clone());
                await!(self.to_index_client.send(AppServerToIndexClient::RequestRoutes(request_routes)))
                    .map_err(|_| AppServerError::SendToIndexClientError)
            },
            AppToAppServer::AddIndexServer(named_index_server_address) => {
                let add_index_server = AddIndexServer {
                    public_key: named_index_server_address.public_key,
                    address: named_index_server_address.address,
                    name: named_index_server_address.name,
                };
                await!(self.to_index_client.send(AppServerToIndexClient::AddIndexServer(add_index_server)))
                    .map_err(|_| AppServerError::SendToIndexClientError)?;
                Ok(())
            },
            AppToAppServer::RemoveIndexServer(index_server_address) =>
                await!(self.to_index_client.send(AppServerToIndexClient::RemoveIndexServer(index_server_address)))
                    .map_err(|_| AppServerError::SendToIndexClientError),
        
        }
    }

    pub async fn handle_from_app(&mut self, app_id: u128, opt_app_message: Option<AppToAppServer<B>>)
        -> Result<(), AppServerError> {

        match opt_app_message {
            None => {
                // Remove the application. We assert that this application exists
                // in our apps map:
                self.apps.remove(&app_id).unwrap();
                if self.apps.is_empty() && self.incoming_connections_closed {
                    return Err(AppServerError::AllAppsClosed);
                }
                Ok(())
            },
            Some(app_message) => await!(self.handle_app_message(app_id, app_message)),
        }
    }
}


#[allow(unused)]
pub async fn app_server_loop<B,FF,TF,FIC,TIC,IC,S>(from_funder: FF, 
                                                       to_funder: TF, 
                                                       from_index_client: FIC,
                                                       to_index_client: TIC,
                                                       incoming_connections: IC,
                                                       initial_node_report: NodeReport<B>,
                                                       mut spawner: S) -> Result<(), AppServerError>
where
    B: Clone + PartialEq + Eq + Debug + Send + Sync + 'static,
    FF: Stream<Item=FunderOutgoingControl<B>> + Unpin,
    TF: Sink<SinkItem=FunderIncomingControl<B>> + Unpin + Sync + Send,
    FIC: Stream<Item=IndexClientToAppServer<B>> + Unpin,
    TIC: Sink<SinkItem=AppServerToIndexClient<B>> + Unpin,
    IC: Stream<Item=IncomingAppConnection<B>> + Unpin,
    S: Spawn,
{

    let (from_app_sender, from_app_receiver) = mpsc::channel(0);
    let mut app_server = AppServer::new(to_funder, 
                                        to_index_client,
                                        from_app_sender,
                                        initial_node_report,
                                        spawner);

    let from_funder = from_funder
        .map(|funder_outgoing_control| AppServerEvent::FromFunder(funder_outgoing_control))
        .chain(stream::once(future::ready(AppServerEvent::FunderClosed)));

    let from_index_client = from_index_client
        .map(|index_client_msg| AppServerEvent::FromIndexClient(index_client_msg))
        .chain(stream::once(future::ready(AppServerEvent::IndexClientClosed)));

    let from_app_receiver = from_app_receiver
        .map(|from_app: (u128, Option<AppToAppServer<B>>)| AppServerEvent::FromApp(from_app));

    let incoming_connections = incoming_connections
        .map(|incoming_connection| AppServerEvent::IncomingConnection(incoming_connection))
        .chain(stream::once(future::ready(AppServerEvent::IncomingConnectionsClosed)));

    let mut events = from_funder
                    .select(from_index_client)
                    .select(from_app_receiver)
                    .select(incoming_connections);

    while let Some(event) = await!(events.next()) {
        match event {
            AppServerEvent::IncomingConnection(incoming_app_connection) =>
                await!(app_server.handle_incoming_connection(incoming_app_connection))?,
            AppServerEvent::IncomingConnectionsClosed =>
                await!(app_server.handle_incoming_connections_closed())?,
            AppServerEvent::FromFunder(funder_outgoing_control) => 
                await!(app_server.handle_from_funder(funder_outgoing_control))?,
            AppServerEvent::FunderClosed => return Err(AppServerError::FunderClosed),
            AppServerEvent::FromIndexClient(from_index_client) => 
                await!(app_server.handle_from_index_client(from_index_client))?,
            AppServerEvent::IndexClientClosed => return Err(AppServerError::IndexClientClosed),
            AppServerEvent::FromApp((app_id, opt_app_message)) => 
                await!(app_server.handle_from_app(app_id, opt_app_message))?,
        }
    }
    Ok(())
}
