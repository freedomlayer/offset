use common::mutable_state::MutableState;

use funder::report::create_initial_report;
use funder::{FunderMutation, FunderState};
use index_client::{IndexClientConfig, IndexClientConfigMutation};

use proto::app_server::messages::NodeReport;
use proto::crypto::PublicKey;
use proto::index_client::messages::IndexClientReport;

use signature::canonical::CanonicalSerialize;

// TODO: Can we remote the Clone bound here?
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NodeMutation<B: Clone> {
    Funder(FunderMutation<B>),
    IndexClient(IndexClientConfigMutation<B>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeState<B: Clone> {
    pub funder_state: FunderState<B>,
    pub index_client_config: IndexClientConfig<B>,
}

impl<B> NodeState<B>
where
    B: Clone + CanonicalSerialize,
{
    pub fn new(local_public_key: PublicKey) -> Self {
        NodeState {
            funder_state: FunderState::new(local_public_key, Vec::new()),
            index_client_config: IndexClientConfig::new(),
        }
    }
}

#[derive(Debug)]
pub struct NodeMutateError;

impl<B> MutableState for NodeState<B>
where
    B: Clone + PartialEq + Eq + CanonicalSerialize,
{
    type Mutation = NodeMutation<B>;
    type MutateError = NodeMutateError;

    fn mutate(&mut self, mutation: &Self::Mutation) -> Result<(), Self::MutateError> {
        match mutation {
            NodeMutation::Funder(funder_mutation) => {
                self.funder_state.mutate(funder_mutation);
                Ok(())
            }
            NodeMutation::IndexClient(index_client_mutation) => self
                .index_client_config
                .mutate(index_client_mutation)
                .map_err(|_| NodeMutateError),
        }
    }
}

/// Create an initial IndexClientReport, based on an IndexClientConfig
fn create_index_client_report<B>(index_client_config: &IndexClientConfig<B>) -> IndexClientReport<B>
where
    B: Clone,
{
    IndexClientReport {
        index_servers: index_client_config.index_servers.clone(),
        // Initially we are not connected to a server:
        opt_connected_server: None,
    }
}

/// Create an initial NodeReport, based on a NodeState
pub fn create_node_report<B>(node_state: &NodeState<B>) -> NodeReport<B>
where
    B: Clone + CanonicalSerialize,
{
    NodeReport {
        funder_report: create_initial_report(&node_state.funder_state),
        index_client_report: create_index_client_report(&node_state.index_client_config),
    }
}

// TODO: Possibly separate NodeConfig into parts that are protocol related, and parts that are
// implementation related?
#[derive(Debug, Clone)]
pub struct NodeConfig {
    /// Memory allocated to a channel in memory (Used to connect two components)
    pub channel_len: usize,
    /// The amount of ticks we wait before attempting to reconnect
    pub backoff_ticks: usize,
    /// The amount of ticks we wait until we decide an idle connection has timed out.
    pub keepalive_ticks: usize,
    /// Amount of ticks to wait until the next rekeying (Channel encryption)
    pub ticks_to_rekey: usize,
    /// Maximum amount of encryption set ups (diffie hellman) that we allow to occur at the same
    /// time from external communications (Channeler side)
    pub max_concurrent_encrypt: usize,
    /// The amount of ticks we are willing to wait until a connection is established.
    pub conn_timeout_ticks: usize,
    /// Maximum amount of operations in one move token message
    pub max_operations_in_batch: usize,
    /// The size we allocate for the user send funds requests queue.
    pub max_pending_user_requests: usize,
    /// Maximum amount of concurrent index client requests:
    pub max_open_index_client_requests: usize,
    /// Maximum amount of relays a node may use.
    pub max_node_relays: usize,
    /// Maximum amount of encryption set ups we allow to occur at the same time
    /// for incoming app connections
    pub max_concurrent_incoming_apps: usize,
}
