use common::canonical_serialize::CanonicalSerialize;
use common::mutable_state::MutableState;

use crypto::identity::PublicKey;
use funder::{FunderState, FunderMutation};
use funder::report::create_initial_report;
use index_client::{IndexClientConfig, IndexClientConfigMutation};

use proto::app_server::messages::NodeReport;
use proto::index_client::messages::IndexClientReport;

#[derive(Clone)]
pub enum NodeMutation<B,ISA> {
    Funder(FunderMutation<Vec<B>>),
    IndexClient(IndexClientConfigMutation<ISA>)
}

#[derive(Clone)]
pub struct NodeState<B: Clone,ISA> {
    pub funder_state: FunderState<Vec<B>>,
    pub index_client_config: IndexClientConfig<ISA>,
}

impl<B,ISA> NodeState<B,ISA> 
where
    B: Clone + CanonicalSerialize,
{
    pub fn new(local_public_key: PublicKey) -> Self {
        let relay_addresses = Vec::new();
        NodeState {
            funder_state: FunderState::new(&local_public_key, &relay_addresses),
            index_client_config: IndexClientConfig::new(),
        }
    }
}


#[derive(Debug)]
pub struct NodeMutateError;

impl<B,ISA> MutableState for NodeState<B,ISA> 
where
    B: Clone + CanonicalSerialize,
    ISA: Clone + PartialEq + Eq,
{
    type Mutation = NodeMutation<B,ISA>;
    type MutateError = NodeMutateError;

    fn mutate(&mut self, mutation: &Self::Mutation) -> Result<(), Self::MutateError> {
        match mutation {
            NodeMutation::Funder(funder_mutation) => {
                self.funder_state.mutate(funder_mutation);
                Ok(())
            },
            NodeMutation::IndexClient(index_client_mutation) => {
                self.index_client_config.mutate(index_client_mutation)
                    .map_err(|_| NodeMutateError)
            },
        }
    }
}


/// Create an initial IndexClientReport, based on an IndexClientConfig
fn create_index_client_report<ISA>(index_client_config: &IndexClientConfig<ISA>) -> IndexClientReport<ISA>
where
    ISA: Clone,
{
    IndexClientReport {
        index_servers: index_client_config.index_servers.clone(),
        // Initially we are not connected to a server:
        opt_connected_server: None,
    }
}


/// Create an initial NodeReport, based on a NodeState
pub fn create_node_report<B,ISA>(node_state: &NodeState<B,ISA>) -> NodeReport<B,ISA> 
where
    B: Clone + CanonicalSerialize,
    ISA: Clone,
{
    NodeReport {
        funder_report: create_initial_report(&node_state.funder_state),
        index_client_report: create_index_client_report(&node_state.index_client_config),
    }
}


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
    /// time.
    pub max_concurrent_encrypt: usize,
    /// The amount of ticks we are willing to wait until a connection is established.
    pub conn_timeout_ticks: usize,
    /// Maximum amount of operations in one move token message
    pub max_operations_in_batch: usize,
    /// The size we allocate for the user send funds requests queue.
    pub max_pending_user_requests: usize,
    /// Maximum amount of concurrent index client requests:
    pub max_open_index_client_requests: usize,
}

