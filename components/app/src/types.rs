use common::mutable_state::MutableState;
use proto::app_server::messages::{NodeReport, NodeReportMutateError, ReportMutations};

/// Given a type which is a MutableState, we convert it to a type
/// that is a MutableState for batches of mutations of the same mutation type.
#[derive(Debug, Clone)]
pub struct BatchNodeReport(pub NodeReport);

impl MutableState for BatchNodeReport {
    type Mutation = ReportMutations;
    type MutateError = NodeReportMutateError;

    fn mutate(&mut self, node_report_mutations: &Self::Mutation) -> Result<(), Self::MutateError> {
        for mutation in &node_report_mutations.mutations {
            self.0.mutate(mutation)?;
        }
        Ok(())
    }
}
