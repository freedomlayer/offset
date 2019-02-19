use common::mutable_state::BatchMutable;
use common::state_service::StateClient;
use proto::app_server::messages::{NodeReport, NodeReportMutation};

#[derive(Clone)]
pub struct AppReport {
    report_client: StateClient<BatchMutable<NodeReport>,Vec<NodeReportMutation>>,
}


