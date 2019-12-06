use common::conn::BoxFuture;
use database::DatabaseClient;

use app::common::{NetAddress, PrivateKey, PublicKey};
use node::{NodeMutation, NodeState};

use identity::IdentityClient;

use crate::compact_node::CompactState;
use crate::messages::{NodeName, NodesInfo};

#[derive(Debug, Clone)]
pub struct LoadedNodeLocal {
    pub node_identity: IdentityClient,
    pub node_state: NodeState<NetAddress>,
    pub node_db_client: DatabaseClient<NodeMutation<NetAddress>>,
}

#[derive(Debug, Clone)]
pub struct LoadedNodeRemote {
    pub app_identity: IdentityClient,
    pub node_public_key: PublicKey,
    pub node_address: NetAddress,
}

#[allow(unused)]
#[derive(Debug, Clone)]
pub enum LoadedNodeType {
    Local(LoadedNodeLocal),
    Remote(LoadedNodeRemote),
}

#[derive(Debug, Clone)]
pub struct LoadedNode {
    pub node_name: NodeName,
    pub compact_state: CompactState,
    pub compact_db_client: DatabaseClient<CompactState>,
    pub node_type: LoadedNodeType,
}

// TODO: Possibly implement encryption for nodes' private key here:
/// Persistent storage manager for nodes' private information.
pub trait Store {
    type Error;

    fn create_local_node<'a>(
        &'a mut self,
        node_name: NodeName,
        node_private_key: PrivateKey,
    ) -> BoxFuture<'a, Result<(), Self::Error>>;

    fn create_remote_node<'a>(
        &'a mut self,
        node_name: NodeName,
        app_private_key: PrivateKey,
        node_public_key: PublicKey,
        node_address: NetAddress,
    ) -> BoxFuture<'a, Result<(), Self::Error>>;

    /// List all existing nodes in store
    fn list_nodes<'a>(&'a self) -> BoxFuture<'a, Result<NodesInfo, Self::Error>>;

    /// Load (private) information of one node
    fn load_node<'a>(
        &'a mut self,
        node_name: NodeName,
    ) -> BoxFuture<'a, Result<LoadedNode, Self::Error>>;

    /// Unload a node
    fn unload_node<'a>(&'a mut self, node_name: NodeName)
        -> BoxFuture<'a, Result<(), Self::Error>>;

    /// Remove a node from the store
    /// A node must be in unloaded state to be removed.
    fn remove_node<'a>(&'a mut self, node_name: NodeName)
        -> BoxFuture<'a, Result<(), Self::Error>>;
}
