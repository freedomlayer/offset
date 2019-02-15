use futures::channel::mpsc;
use futures::task::Spawn;

use proto::app_server::messages::{AppToAppServer, 
    AppPermissions, NodeReport, NodeReportMutation};
use common::state_service::StateClient;

use crate::connect::NodeConnectionTuple;

struct ReportRequest;

#[derive(Debug)]
pub enum NodeConnectionError {
    SpawnError,
}

pub struct NodeConnection {
    sender: mpsc::Sender<AppToAppServer>,
    report_client: StateClient<NodeReport,NodeReportMutation>,
    app_permissions: AppPermissions,
}

// TODO:
pub struct AppConfig;
pub struct AppRoutes;
pub struct AppSendFunds;

impl NodeConnection {
    pub fn new<S>(conn_tuple: NodeConnectionTuple, spawner: &mut S) 
        -> Result<Self, NodeConnectionError> 
    where
        S: Spawn,
    {

        let (app_permissions, (sender, receiver)) = conn_tuple;
        unimplemented!();

    }

    pub async fn report() -> Option<(NodeReport, mpsc::Receiver<NodeReportMutation>)> {
        unimplemented!();
    }

    pub fn config() -> Option<AppConfig> {
        unimplemented!();
    }

    pub fn routes() -> Option<AppRoutes> {
        unimplemented!();
    }

    pub fn send_funds() -> Option<AppSendFunds> {
        unimplemented!();
    }
}
