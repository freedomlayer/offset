use futures::channel::mpsc;

use proto::app_server::messages::{AppToAppServer, 
    AppPermissions, NodeReport, NodeReportMutation};

struct ReportRequest;

pub struct NodeConnection {
    sender: mpsc::Sender<AppToAppServer>,
    request_report_sender: mpsc::Sender<ReportRequest>,
    app_permissions: AppPermissions,
}

// TODO:
pub struct AppConfig;
pub struct AppRoutes;
pub struct AppSendFunds;

impl NodeConnection {
    pub fn report() -> Option<(NodeReport, mpsc::Receiver<NodeReportMutation>)> {
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
