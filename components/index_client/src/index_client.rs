use std::marker::Unpin;

use futures::{future, stream, Stream, StreamExt, Sink, SinkExt};
use futures::channel::mpsc;
use futures::task::Spawn;

use common::conn::FutTransform;

use crypto::identity::PublicKey;
use crypto::uid::Uid;

use proto::index_client::messages::{AppServerToIndexClient, IndexClientToAppServer,
                                    IndexClientMutation, IndexClientState,
                                    ResponseRoutesResult};

use crate::client_session::{SessionHandle, ControlSender, CloseReceiver};

pub struct IndexClientConfig<ISA> {
    index_servers: Vec<ISA>,
}

pub enum IndexClientConfigMutation<ISA> {
    AddIndexServer(ISA),
    RemoveIndexServer(ISA),
}


#[derive(Debug)]
struct ConnectedIndexServer<ISA> {
    address: ISA,
    opt_control_sender: Option<ControlSender>,
}

#[derive(Debug)]
pub enum IndexClientError {
    AppServerClosed,
}

#[derive(Debug)]
enum IndexClientEvent<ISA> {
    FromAppServer(AppServerToIndexClient<ISA>),
    AppServerClosed,
    IndexServerConnected((ISA, ControlSender)),
    IndexServerClosed(ISA),
    ResponseRoutes((Uid, ResponseRoutesResult)),
}

struct IndexClient<ISA,TAS,DB,S> {
    event_sender: mpsc::Sender<IndexClientEvent<ISA>>,
    to_app_server: TAS,
    index_client_config: IndexClientConfig<ISA>,
    index_client_state: IndexClientState,
    max_open_requests: usize,
    num_open_requests: usize,
    opt_connected_index_server: Option<ConnectedIndexServer<ISA>>,
    database: DB,
    spawner: S,
}

impl<ISA,TAS,DB,S> IndexClient<ISA,TAS,DB,S> 
where
    TAS: Sink<SinkItem=IndexClientToAppServer<ISA>>,
    DB: FutTransform<Input=IndexClientConfigMutation<ISA>, Output=Option<()>>,
    S: Spawn,
{
    pub fn new(event_sender: mpsc::Sender<IndexClientEvent<ISA>>,
               to_app_server: TAS,
               index_client_config: IndexClientConfig<ISA>,
               index_client_state: IndexClientState,
               max_open_requests: usize,
               database: DB,
               spawner: S) -> Self { 

        IndexClient {
            event_sender,
            to_app_server,
            index_client_config,
            index_client_state,
            max_open_requests,
            num_open_requests: 0,
            opt_connected_index_server: None,
            database,
            spawner,
        }
    }

    pub fn handle_from_app_server(&mut self, 
                                  app_server_to_index_client: AppServerToIndexClient<ISA>) {
        unimplemented!();
    }

    pub fn handle_index_server_connected(&mut self, address: ISA, control_sender: ControlSender) {

        if let Some(_) = self.opt_connected_index_server {
            error!("Index server already connected!");
            return;
        }

        self.opt_connected_index_server = Some(ConnectedIndexServer { 
            address, 
            opt_control_sender: Some(control_sender)
        });

        self.num_open_requests = 0;
    }

    pub fn handle_index_server_closed(&mut self, address: ISA) {
        unimplemented!();
    }

    pub async fn handle_response_routes(&mut self, request_id: Uid, 
                                        response_routes_result: ResponseRoutesResult) 
                                            -> Result<(), IndexClientError> {
        unimplemented!();
    }
}

pub async fn index_client_loop<ISA,A,FAS,TAS,ICS,DB,S>(from_app_server: FAS,
                               to_app_server: TAS,
                               mut index_client_config: IndexClientConfig<ISA>,
                               index_client_state: IndexClientState,
                               index_client_session: ICS,
                               max_open_requests: usize,
                               database: DB,
                               spawner: S) -> Result<(), IndexClientError>
where
    FAS: Stream<Item=AppServerToIndexClient<ISA>> + Unpin,
    TAS: Sink<SinkItem=IndexClientToAppServer<ISA>>,
    ICS: FutTransform<Input=ISA, Output=Option<SessionHandle>>,
    DB: FutTransform<Input=IndexClientConfigMutation<ISA>, Output=Option<()>>,
    S: Spawn,
{
    let (event_sender, event_receiver) = mpsc::channel(0);
    let mut index_client = IndexClient::new(event_sender, 
                                            to_app_server,
                                            index_client_config,
                                            index_client_state,
                                            max_open_requests, 
                                            database,
                                            spawner);

    let from_app_server = from_app_server
        .map(|app_server_to_index_client| IndexClientEvent::FromAppServer(app_server_to_index_client))
        .chain(stream::once(future::ready(IndexClientEvent::AppServerClosed)));

    let mut events = event_receiver
        .select(from_app_server);

    while let Some(event) = await!(events.next()) {
        match event {
            IndexClientEvent::FromAppServer(app_server_to_index_client) =>
                index_client.handle_from_app_server(app_server_to_index_client),
            IndexClientEvent::AppServerClosed => return Err(IndexClientError::AppServerClosed),
            IndexClientEvent::IndexServerConnected((address, control_sender)) => 
                index_client.handle_index_server_connected(address, control_sender),
            IndexClientEvent::IndexServerClosed(address) =>
                index_client.handle_index_server_closed(address),
            IndexClientEvent::ResponseRoutes((request_id, response_routes_result)) =>
                await!(index_client.handle_response_routes(request_id, response_routes_result))?,
        };
    }
    Ok(())
}
