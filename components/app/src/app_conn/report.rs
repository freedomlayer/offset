use futures::channel::mpsc;

use crate::types::BatchNodeReport;
use common::state_service::StateClient;
use proto::app_server::messages::{NodeReport, ReportMutations};

#[derive(Debug)]
pub struct AppReportError;

#[derive(Clone)]
pub struct AppReport {
    report_client: StateClient<BatchNodeReport, ReportMutations>,
}

impl AppReport {
    // TODO: Should this be private?
    pub(super) fn new(
        report_client: StateClient<BatchNodeReport, ReportMutations>,
    ) -> Self {
        AppReport { report_client }
    }

    pub async fn incoming_reports(
        &mut self,
    ) -> Result<(NodeReport, mpsc::Receiver<ReportMutations>), AppReportError> {
        let (batch_node_report, incoming_mutations) = self
            .report_client
            .request_state()
            .await
            .map_err(|_| AppReportError)?;

        Ok((batch_node_report.0, incoming_mutations))
    }
}
