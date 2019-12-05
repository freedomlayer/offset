use serde::{Deserialize, Serialize};

use common::conn::BoxFuture;
use database::DatabaseClient;

use app::common::{NetAddress, PrivateKey, PublicKey};
use node::{NodeMutation, NodeState};

use crate::compact_node::CompactState;
use crate::messages::{NodeInfo, NodeName};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodePrivateInfoLocal {
    pub node_name: NodeName,
    pub node_private_key: PrivateKey,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodePrivateInfoRemote {
    pub node_name: NodeName,
    pub app_private_key: PrivateKey,
    pub node_public_key: PublicKey,
    pub node_address: NetAddress,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NodePrivateInfo {
    Local(NodePrivateInfoLocal),
    Remote(NodePrivateInfoRemote),
}

#[derive(Debug, Clone)]
pub struct LoadedNodeRemote {
    pub node_state: NodeState<NetAddress>,
    pub node_db_client: DatabaseClient<NodeMutation<NetAddress>>,
}

#[allow(unused)]
#[derive(Debug, Clone)]
pub enum LoadedNodeType {
    Local,
    Remote(LoadedNodeRemote),
}

#[derive(Debug, Clone)]
pub struct LoadedNode {
    pub private_info: NodePrivateInfo,
    pub compact_state: CompactState,
    pub compact_db_client: DatabaseClient<CompactState>,
    pub node_type: LoadedNodeType,
}

// TODO: Possibly implement encryption for nodes' private key here:
/// Persistent storage manager for nodes' private information.
pub trait Store {
    type Error;

    /// Create a new node
    fn create_node(
        node_private_info: NodePrivateInfo,
    ) -> BoxFuture<'static, Result<(), Self::Error>>;

    /// List all existing nodes in store
    fn list_nodes(&self) -> BoxFuture<'static, Result<Vec<NodeInfo>, Self::Error>>;

    /// Load (private) information of one node
    fn load_node(
        &mut self,
        node_name: &NodeName,
    ) -> BoxFuture<'static, Result<LoadedNode, Self::Error>>;

    /// Unload a node
    fn unload_node(&mut self, node_name: &NodeName) -> BoxFuture<'static, Result<(), Self::Error>>;

    /// Remove a node from the store
    /// A node must be in unloaded state to be removed.
    fn remove_node(&mut self, node_name: &NodeName) -> BoxFuture<'static, Result<(), Self::Error>>;
}
