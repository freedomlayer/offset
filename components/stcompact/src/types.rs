use common::conn::ConnPair;

use app::conn::AppServerToApp;
use app::common::{Uid, PaymentId};
use database::DatabaseClient;

use crate::persist::CompactState;
use crate::messages::{ToUser, FromUser};

pub trait GenId {
    /// Generate a Uid
    fn gen_uid(&mut self) -> Uid;

    /// Generate a PaymentId
    fn gen_payment_id(&mut self) -> PaymentId;
}

pub type ConnPairCompact = ConnPair<ToUser, FromUser>;


#[derive(Debug)]
pub enum CompactServerEvent {
    User(FromUser),
    UserClosed,
    Node(AppServerToApp),
    NodeClosed,
}

pub enum CompactServerError {
    AppSenderError,
    UserSenderError,
    ReportMutationError,
    DatabaseMutateError,
}

pub struct CompactServerState {
    node_report: app::report::NodeReport,
    compact_state: CompactState,
    database_client: DatabaseClient<CompactState>,
}

impl CompactServerState {
    pub fn new(node_report: app::report::NodeReport, compact_state: CompactState, database_client: DatabaseClient<CompactState>) -> Self {
        Self {
            node_report,
            compact_state,
            database_client,
        }
    }

    /// Get current `node_update`
    pub fn node_report(&self) -> &app::report::NodeReport {
        &self.node_report
    }

    pub fn update_node_report(&mut self, node_report: app::report::NodeReport) {
        self.node_report = node_report;
    }

    /// Get current `compact_state`
    pub fn compact_state(&self) -> &CompactState {
        &self.compact_state
    }

    /// Persistent (and atomic) update to `compact_state`
    pub async fn update_compact_state(&mut self, compact_state: CompactState) -> Result<(), CompactServerError> {
        self.compact_state = compact_state.clone();
        self.database_client.mutate(vec![compact_state])
            .await
            .map_err(|_| CompactServerError::DatabaseMutateError)?;
        Ok(())
    }
}
