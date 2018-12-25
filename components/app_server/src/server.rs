use std::marker::Unpin;
use std::collections::HashMap;

use futures::{future, FutureExt, stream, Stream, StreamExt, Sink, SinkExt};
use futures::channel::mpsc;
use futures::task::{Spawn, SpawnExt};

use common::conn::ConnPair;
use crypto::identity::PublicKey;

use proto::funder::messages::{FunderOutgoingControl, FunderIncomingControl};
use proto::app_server::messages::{AppServerToApp, AppToAppServer};

use crate::config::AppServerConfig;

type IncomingAppConnection<A> = (PublicKey, ConnPair<AppServerToApp<A>, AppToAppServer<A>>);


pub enum AppServerError {
    FunderClosed,
    SpawnError,
}

pub enum AppServerEvent<A: Clone> {
    IncomingConnection(IncomingAppConnection<A>),
    FromFunder(FunderOutgoingControl<A>),
    FunderClosed,
    FromApp((u128, Option<AppToAppServer<A>>)), // None means that app was closed
}

pub struct App<A: Clone> {
    public_key: PublicKey,
    opt_sender: Option<mpsc::Sender<AppServerToApp<A>>>,
}

pub struct AppServer<A: Clone,TF> {
    to_funder: TF,
    from_app_sender: mpsc::Sender<(u128, Option<AppToAppServer<A>>)>,
    config: AppServerConfig,
    /// A long cyclic incrementing counter, 
    /// allowing to give every connection a unique number.
    /// Required because an app (with one public key) might have multiple connections.
    app_counter: u128,
    apps: HashMap<u128, App<A>>,
}

impl<A,TF> AppServer<A,TF> 
where
    A: Clone,
{
    pub fn new(to_funder: TF, 
           from_app_sender: mpsc::Sender<(u128, Option<AppToAppServer<A>>)>,
           config: AppServerConfig) -> AppServer<A,TF> {

        AppServer {
            to_funder,
            from_app_sender,
            config,
            app_counter: 0,
            apps: HashMap::new(),
        }
    }

    /// Add an application connection
    pub fn add_app_connection<S>(&mut self, 
                                 incoming_app_connection: IncomingAppConnection<A>,
                                 spawner: &mut S) -> Result<(), AppServerError>
    where
        S: Spawn,
        TF: Sync,
        A: Clone + Send + 'static,
    {
        let (public_key, (sender, mut receiver)) = incoming_app_connection;

        let app_counter = self.app_counter;
        let mut receiver = receiver.map(move |app_to_app_server| (app_counter.clone(), Some(app_to_app_server)))
            .chain(stream::once(future::ready((app_counter, None))));

        let mut from_app_sender = self.from_app_sender.clone();
        spawner.spawn(async move {
            let _ = await!(from_app_sender.send_all(&mut receiver));
        }).map_err(|_| AppServerError::SpawnError)?;

        let app = App {
            public_key,
            opt_sender: Some(sender),
        };

        self.apps.insert(self.app_counter, app);
        self.app_counter = self.app_counter.wrapping_add(1);
        Ok(())
    }
}


pub async fn app_server_loop<A,FF,TF,IC,S>(config: AppServerConfig, 
                                   from_funder: FF, to_funder: TF, 
                                   incoming_connections: IC,
                                   mut spawner: S) -> Result<(), AppServerError>
where
    A: Clone + Send + 'static,
    FF: Stream<Item=FunderOutgoingControl<A>> + Unpin,
    TF: Sink<SinkItem=FunderIncomingControl<A>> + Unpin + Sync + Send,
    IC: Stream<Item=IncomingAppConnection<A>> + Unpin,
    S: Spawn,
{

    let (from_app_sender, from_app_receiver) = mpsc::channel(0);
    let mut app_server = AppServer::new(to_funder, from_app_sender, config);

    let from_funder = from_funder
        .map(|funder_outgoing_control| AppServerEvent::FromFunder(funder_outgoing_control))
        .chain(stream::once(future::ready(AppServerEvent::FunderClosed)));

    let from_app_receiver = from_app_receiver
        .map(|from_app: (u128, Option<AppToAppServer<A>>)| AppServerEvent::FromApp(from_app));

    let incoming_connections = incoming_connections
        .map(|incoming_connection| AppServerEvent::IncomingConnection(incoming_connection));

    let mut events = from_funder
                    .select(from_app_receiver)
                    .select(incoming_connections);

    while let Some(event) = await!(events.next()) {
        match event {
            AppServerEvent::IncomingConnection(incoming_app_connection) => {
                app_server.add_app_connection(incoming_app_connection, &mut spawner)?;
            },
            AppServerEvent::FromFunder(funder_outgoing_control) => unimplemented!(),
            AppServerEvent::FunderClosed => return Err(AppServerError::FunderClosed),
            AppServerEvent::FromApp((conn_id, Some(funder_incoming_control))) => unimplemented!(),
            AppServerEvent::FromApp((conn_id, None)) => {
                // App connection closed:
                app_server.apps.remove(&conn_id).unwrap();
            }
        }
        // TODO
    }
    
    unimplemented!();
}
