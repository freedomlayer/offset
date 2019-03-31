use futures::channel::mpsc;
use futures::task::{Spawn, SpawnExt};
use futures::{FutureExt, SinkExt, StreamExt, TryFutureExt};

use proto::app_server::messages::AppServerToApp;

use crypto::crypto_rand::{CryptoRandom, OffstSystemRandom};

use common::multi_consumer::{multi_consumer_service, MultiConsumerClient};
use common::mutable_state::BatchMutable;
use common::state_service::{state_service, StateClient};

use super::config::AppConfig;
use super::report::AppReport;
use super::routes::AppRoutes;
use super::send_funds::AppSendFunds;

use crate::connect::connect::NodeConnectionTuple;

#[derive(Debug)]
pub enum NodeConnectionError {
    SpawnError,
}

// TODO: Do we need a way to close this connection?
// Is it closed on Drop?
#[derive(Clone)]
pub struct NodeConnection<R = OffstSystemRandom> {
    report: AppReport,
    opt_config: Option<AppConfig<R>>,
    opt_routes: Option<AppRoutes<R>>,
    opt_send_funds: Option<AppSendFunds<R>>,
    rng: R,
}

impl<R> NodeConnection<R>
where
    R: CryptoRandom + Clone,
{
    pub fn new<S>(
        conn_tuple: NodeConnectionTuple,
        rng: R,
        spawner: &mut S,
    ) -> Result<Self, NodeConnectionError>
    where
        S: Spawn,
    {
        let (app_permissions, node_report, (sender, mut receiver)) = conn_tuple;

        let (mut incoming_mutations_sender, incoming_mutations) = mpsc::channel(0);
        let (requests_sender, incoming_requests) = mpsc::channel(0);
        let report_client = StateClient::new(requests_sender);
        let state_service_fut = state_service(
            incoming_requests,
            BatchMutable(node_report),
            incoming_mutations,
        )
        .map_err(|e| error!("state_service() error: {:?}", e))
        .map(|_| ());
        spawner
            .spawn(state_service_fut)
            .map_err(|_| NodeConnectionError::SpawnError)?;

        let (mut incoming_routes_sender, incoming_routes) = mpsc::channel(0);
        let (requests_sender, incoming_requests) = mpsc::channel(0);
        let routes_mc = MultiConsumerClient::new(requests_sender);
        let routes_fut = multi_consumer_service(incoming_routes, incoming_requests)
            .map_err(|e| error!("Routes multi_consumer_service() error: {:?}", e))
            .map(|_| ());
        spawner
            .spawn(routes_fut)
            .map_err(|_| NodeConnectionError::SpawnError)?;

        let (mut incoming_send_funds_sender, incoming_send_funds) = mpsc::channel(0);
        let (requests_sender, incoming_requests) = mpsc::channel(0);
        let send_funds_mc = MultiConsumerClient::new(requests_sender);
        let send_funds_fut = multi_consumer_service(incoming_send_funds, incoming_requests)
            .map_err(|e| error!("SendFunds multi_consumer_service() error: {:?}", e))
            .map(|_| ());
        spawner
            .spawn(send_funds_fut)
            .map_err(|_| NodeConnectionError::SpawnError)?;

        let (mut incoming_done_app_requests_sender, incoming_done_app_requests) = mpsc::channel(0);
        let (requests_sender, incoming_requests) = mpsc::channel(0);
        let done_app_requests_mc = MultiConsumerClient::new(requests_sender);
        let done_app_requests_fut =
            multi_consumer_service(incoming_done_app_requests, incoming_requests)
                .map_err(|e| error!("DoneAppRequests multi_consumer_service() error: {:?}", e))
                .map(|_| ());
        spawner
            .spawn(done_app_requests_fut)
            .map_err(|_| NodeConnectionError::SpawnError)?;

        spawner
            .spawn(async move {
                while let Some(message) = await!(receiver.next()) {
                    match message {
                        AppServerToApp::ResponseReceived(response_received) => {
                            let _ = await!(incoming_send_funds_sender.send(response_received));
                        }
                        AppServerToApp::Report(_node_report) => {
                            // TODO: Maybe somehow redesign the type AppServerToApp
                            // so that we don't have this edge case?
                            error!("Received unexpected AppServerToApp::Report message. Aborting.");
                            return;
                        }
                        AppServerToApp::ReportMutations(node_report_mutations) => {
                            let _ = await!(
                                incoming_mutations_sender.send(node_report_mutations.mutations)
                            );
                            if let Some(app_request_id) = node_report_mutations.opt_app_request_id {
                                let _ =
                                    await!(incoming_done_app_requests_sender.send(app_request_id));
                            }
                        }
                        AppServerToApp::ResponseRoutes(client_response_routes) => {
                            let _ = await!(incoming_routes_sender.send(client_response_routes));
                        }
                    }
                }
            })
            .map_err(|_| NodeConnectionError::SpawnError)?;

        let opt_config = if app_permissions.config {
            Some(AppConfig::new(
                sender.clone(),
                done_app_requests_mc.clone(),
                rng.clone(),
            ))
        } else {
            None
        };

        let opt_routes = if app_permissions.routes {
            Some(AppRoutes::new(
                sender.clone(),
                routes_mc.clone(),
                rng.clone(),
            ))
        } else {
            None
        };

        let opt_send_funds = if app_permissions.send_funds {
            Some(AppSendFunds::new(
                sender.clone(),
                send_funds_mc.clone(),
                done_app_requests_mc.clone(),
                rng.clone(),
            ))
        } else {
            None
        };

        Ok(NodeConnection {
            report: AppReport::new(report_client.clone()),
            opt_config,
            opt_routes,
            opt_send_funds,
            rng,
        })
    }

    pub fn report(&mut self) -> &mut AppReport {
        &mut self.report
    }

    pub fn config(&mut self) -> Option<&mut AppConfig<R>> {
        self.opt_config.as_mut()
    }

    pub fn routes(&mut self) -> Option<&mut AppRoutes<R>> {
        self.opt_routes.as_mut()
    }

    pub fn send_funds(&mut self) -> Option<&mut AppSendFunds<R>> {
        self.opt_send_funds.as_mut()
    }
}
