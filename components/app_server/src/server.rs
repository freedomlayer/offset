use std::marker::Unpin;
use std::collections::HashMap;

use futures::{future, FutureExt, stream, Stream, StreamExt, Sink, SinkExt};
use futures::channel::mpsc;
use futures::task::{Spawn, SpawnExt};

use common::conn::ConnPair;
use crypto::identity::PublicKey;

use proto::funder::messages::{FunderOutgoingControl, FunderIncomingControl};
use proto::app_server::messages::{AppServerToApp, AppToAppServer};
use proto::index_client::messages::{IndexClientToAppServer, AppServerToIndexClient};

use crate::config::AppPermissions;

type IncomingAppConnection<B,ISA> = (PublicKey, AppPermissions, ConnPair<AppServerToApp<B,ISA>, AppToAppServer<B,ISA>>);


pub enum AppServerError {
    FunderClosed,
    SpawnError,
    IndexClientClosed,
}

pub enum AppServerEvent<B: Clone,ISA> {
    IncomingConnection(IncomingAppConnection<B,ISA>),
    FromFunder(FunderOutgoingControl<Vec<B>>),
    FunderClosed,
    FromIndexClient(IndexClientToAppServer<ISA>),
    IndexClientClosed,
    FromApp((u128, Option<AppToAppServer<B,ISA>>)), // None means that app was closed
}

pub struct App<B: Clone,ISA> {
    public_key: PublicKey,
    permissions: AppPermissions,
    opt_sender: Option<mpsc::Sender<AppServerToApp<B,ISA>>>,
}

pub struct AppServer<B: Clone,ISA,TF> {
    to_funder: TF,
    from_app_sender: mpsc::Sender<(u128, Option<AppToAppServer<B,ISA>>)>,
    /// A long cyclic incrementing counter, 
    /// allows to give every connection a unique number.
    /// Required because an app (with one public key) might have multiple connections.
    app_counter: u128,
    apps: HashMap<u128, App<B,ISA>>,
}

impl<B,ISA,TF> AppServer<B,ISA,TF> 
where
    B: Clone + Send + 'static,
    ISA: Send + 'static,
    TF: Sync,
{
    pub fn new(to_funder: TF, 
           from_app_sender: mpsc::Sender<(u128, Option<AppToAppServer<B,ISA>>)>) -> Self {

        AppServer {
            to_funder,
            from_app_sender,
            app_counter: 0,
            apps: HashMap::new(),
        }
    }

    /// Add an application connection
    pub fn add_app_connection<S>(&mut self, 
                                 incoming_app_connection: IncomingAppConnection<B,ISA>,
                                 spawner: &mut S) -> Result<(), AppServerError>
    where
        S: Spawn,
    {
        let (public_key, permissions, (sender, mut receiver)) = incoming_app_connection;

        let app_counter = self.app_counter;
        let mut receiver = receiver.map(move |app_to_app_server| (app_counter.clone(), Some(app_to_app_server)))
            .chain(stream::once(future::ready((app_counter, None))));

        let mut from_app_sender = self.from_app_sender.clone();
        spawner.spawn(async move {
            let _ = await!(from_app_sender.send_all(&mut receiver));
        }).map_err(|_| AppServerError::SpawnError)?;

        let app = App {
            public_key,
            permissions,
            opt_sender: Some(sender),
        };

        self.apps.insert(self.app_counter, app);
        self.app_counter = self.app_counter.wrapping_add(1);
        Ok(())
    }

    fn handle_from_funder(&mut self, funder_message: FunderOutgoingControl<Vec<B>>)
        -> Result<(), AppServerError> {

        unimplemented!();
    }

    fn handle_from_index_client(&mut self, index_client_message: IndexClientToAppServer<ISA>) 
        -> Result<(), AppServerError> {

        unimplemented!();
    }

    fn handle_from_app(&mut self, app_id: u128, opt_app_message: Option<AppToAppServer<B,ISA>>)
        -> Result<(), AppServerError> {
        // Old code:
        // App connection closed:
        // app_server.apps.remove(&conn_id).unwrap();
        unimplemented!();
    }
}


pub async fn app_server_loop<B,ISA,FF,TF,FIC,TIC,IC,S>(from_funder: FF, 
                                                       to_funder: TF, 
                                                       from_index_client: FIC,
                                                       to_index_client: TIC,
                                                       incoming_connections: IC,
                                                       mut spawner: S) -> Result<(), AppServerError>
where
    B: Clone + Send + 'static,
    ISA: Send + 'static,
    FF: Stream<Item=FunderOutgoingControl<Vec<B>>> + Unpin,
    TF: Sink<SinkItem=FunderIncomingControl<Vec<B>>> + Unpin + Sync + Send,
    FIC: Stream<Item=IndexClientToAppServer<ISA>> + Unpin,
    TIC: Sink<SinkItem=AppServerToIndexClient<ISA>> + Unpin,
    IC: Stream<Item=IncomingAppConnection<B,ISA>> + Unpin,
    S: Spawn,
{

    let (from_app_sender, from_app_receiver) = mpsc::channel(0);
    let mut app_server = AppServer::new(to_funder, from_app_sender);

    let from_funder = from_funder
        .map(|funder_outgoing_control| AppServerEvent::FromFunder(funder_outgoing_control))
        .chain(stream::once(future::ready(AppServerEvent::FunderClosed)));

    let from_index_client = from_index_client
        .map(|index_client_msg| AppServerEvent::FromIndexClient(index_client_msg))
        .chain(stream::once(future::ready(AppServerEvent::IndexClientClosed)));

    let from_app_receiver = from_app_receiver
        .map(|from_app: (u128, Option<AppToAppServer<B,ISA>>)| AppServerEvent::FromApp(from_app));

    let incoming_connections = incoming_connections
        .map(|incoming_connection| AppServerEvent::IncomingConnection(incoming_connection));

    let mut events = from_funder
                    .select(from_index_client)
                    .select(from_app_receiver)
                    .select(incoming_connections);

    while let Some(event) = await!(events.next()) {
        match event {
            AppServerEvent::IncomingConnection(incoming_app_connection) => {
                app_server.add_app_connection(incoming_app_connection, &mut spawner)?;
            },
            AppServerEvent::FromFunder(funder_outgoing_control) => 
                app_server.handle_from_funder(funder_outgoing_control)?,
            AppServerEvent::FunderClosed => return Err(AppServerError::FunderClosed),
            AppServerEvent::FromIndexClient(from_index_client) => 
                app_server.handle_from_index_client(from_index_client)?,
            AppServerEvent::IndexClientClosed => return Err(AppServerError::IndexClientClosed),
            AppServerEvent::FromApp((app_id, opt_app_message)) => 
                app_server.handle_from_app(app_id, opt_app_message)?,
        }
    }
    Ok(())
}
