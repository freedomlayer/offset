use futures::channel::mpsc;

use common::mutable_state::BatchMutable;
use common::state_service::StateClient;
use proto::app_server::messages::{NodeReport, NodeReportMutation};

#[derive(Debug)]
pub struct AppReportError;

#[derive(Clone)]
pub struct AppReport {
    report_client: StateClient<BatchMutable<NodeReport>, Vec<NodeReportMutation>>,
}

impl AppReport {
    // TODO: Should this be private?
    pub(super) fn new(
        report_client: StateClient<BatchMutable<NodeReport>, Vec<NodeReportMutation>>,
    ) -> Self {
        AppReport { report_client }
    }

    pub async fn incoming_reports(
        &mut self,
    ) -> Result<(NodeReport, mpsc::Receiver<Vec<NodeReportMutation>>), AppReportError> {
        let (batch_mutable, incoming_mutations) =
            self.report_client.request_state().await.map_err(|_| AppReportError)?;

        Ok((batch_mutable.0, incoming_mutations))
    }
}
