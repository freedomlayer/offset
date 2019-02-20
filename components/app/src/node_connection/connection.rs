use futures::{FutureExt, TryFutureExt, StreamExt, SinkExt};
use futures::channel::mpsc;
use futures::task::{Spawn, SpawnExt};

use proto::app_server::messages::{AppToAppServer, AppServerToApp,
    AppPermissions, NodeReport, NodeReportMutation};
use proto::funder::messages::ResponseReceived;
use proto::index_client::messages::ClientResponseRoutes;

use crypto::uid::Uid;
use crypto::crypto_rand::CryptoRandom;

use common::state_service::{state_service, StateClient};
use common::mutable_state::BatchMutable;
use common::multi_consumer::{multi_consumer_service, MultiConsumerClient};

use crate::connect::NodeConnectionTuple;

use super::report::AppReport;
use super::config::AppConfig;
use super::routes::AppRoutes;
use super::send_funds::AppSendFunds;

#[derive(Debug)]
pub enum NodeConnectionError {
    SpawnError,
}

#[derive(Clone)]
pub struct NodeConnection<R> {
    sender: mpsc::Sender<AppToAppServer>,
    app_permissions: AppPermissions,
    report_client: StateClient<BatchMutable<NodeReport>,Vec<NodeReportMutation>>,
    routes_mc: MultiConsumerClient<ClientResponseRoutes>,
    send_funds_mc: MultiConsumerClient<ResponseReceived>,
    done_app_requests_mc: MultiConsumerClient<Uid>,
    rng: R,
}

impl<R> NodeConnection<R> 
where
    R: CryptoRandom + Clone,
{
    pub fn new<S>(conn_tuple: NodeConnectionTuple, 
                  rng: R, 
                  spawner: &mut S) 
        -> Result<Self, NodeConnectionError> 
    where
        S: Spawn,
    {
        let (app_permissions, node_report, (sender, mut receiver)) = conn_tuple;

        let (mut incoming_mutations_sender, incoming_mutations) = mpsc::channel(0);
        let (requests_sender, incoming_requests) = mpsc::channel(0);
        let report_client = StateClient::new(requests_sender);
        let state_service_fut = state_service(incoming_requests,
                      BatchMutable(node_report),
                      incoming_mutations)
            .map_err(|e| error!("state_service() error: {:?}", e))
            .map(|_| ());
        spawner.spawn(state_service_fut)
            .map_err(|_| NodeConnectionError::SpawnError);

        let (mut incoming_routes_sender, incoming_routes) = mpsc::channel(0);
        let (requests_sender, incoming_requests) = mpsc::channel(0);
        let routes_mc = MultiConsumerClient::new(requests_sender);
        let routes_fut = multi_consumer_service(incoming_routes, incoming_requests)
            .map_err(|e| error!("Routes multi_consumer_service() error: {:?}", e))
            .map(|_| ());
        spawner.spawn(routes_fut)
                .map_err(|_| NodeConnectionError::SpawnError);

        let (mut incoming_send_funds_sender, incoming_send_funds) = mpsc::channel(0);
        let (requests_sender, incoming_requests) = mpsc::channel(0);
        let send_funds_mc = MultiConsumerClient::new(requests_sender);
        let send_funds_fut = multi_consumer_service(incoming_send_funds, incoming_requests)
            .map_err(|e| error!("SendFunds multi_consumer_service() error: {:?}", e))
            .map(|_| ());
        spawner.spawn(send_funds_fut)
                .map_err(|_| NodeConnectionError::SpawnError);

        let (mut incoming_done_app_requests_sender, incoming_done_app_requests) = mpsc::channel(0);
        let (requests_sender, incoming_requests) = mpsc::channel(0);
        let done_app_requests_mc = MultiConsumerClient::new(requests_sender);
        let done_app_requests_fut = multi_consumer_service(incoming_done_app_requests, incoming_requests)
            .map_err(|e| error!("DoneAppRequests multi_consumer_service() error: {:?}", e))
            .map(|_| ());
        spawner.spawn(done_app_requests_fut)
                .map_err(|_| NodeConnectionError::SpawnError);
        
        async move {
            while let Some(message) = await!(receiver.next()) {
                match message {
                    AppServerToApp::ResponseReceived(response_received) => {
                        let _ = await!(incoming_send_funds_sender.send(response_received));
                    },
                    AppServerToApp::Report(_node_report) => {
                        // TODO: Maybe somehow redesign the type AppServerToApp
                        // so that we don't have this edge case?
                        error!("Received unexpected AppServerToApp::Report message. Aborting.");
                        return;
                    },
                    AppServerToApp::ReportMutations(node_report_mutations) => {
                        let _ = await!(incoming_mutations_sender.send(node_report_mutations.mutations));
                        if let Some(app_request_id) = node_report_mutations.opt_app_request_id {
                            let _ = await!(incoming_done_app_requests_sender.send(app_request_id));
                        }
                    },
                    AppServerToApp::ResponseRoutes(client_response_routes) => {
                        let _ = await!(incoming_routes_sender.send(client_response_routes));
                    },
                }
            }
        };

        Ok(NodeConnection {
            sender,
            app_permissions,
            report_client,
            routes_mc,
            send_funds_mc,
            done_app_requests_mc,
            rng,
        })
    }

    pub fn report(&self) -> AppReport {
        AppReport::new(self.report_client.clone())
    }

    pub fn config(&self) -> Option<AppConfig<R>> {
        if !self.app_permissions.config {
            return None;
        }
        Some(AppConfig::new(self.sender.clone(),
                           self.done_app_requests_mc.clone(),
                           self.rng.clone()))
    }

    pub fn routes(&self) -> Option<AppRoutes<R>> {
        if !self.app_permissions.routes {
            return None;
        }
        Some(AppRoutes::new(self.sender.clone(),
                            self.routes_mc.clone(),
                            self.rng.clone()))
    }

    pub fn send_funds(&self) -> Option<AppSendFunds<R>> {
        if !self.app_permissions.send_funds {
            return None;
        }
        Some(AppSendFunds::new(self.sender.clone(),
                                self.send_funds_mc.clone(),
                                self.done_app_requests_mc.clone(),
                                self.rng.clone()))
    }
}
