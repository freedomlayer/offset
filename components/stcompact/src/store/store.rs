use std::collections::HashMap;
use std::fmt::Debug;

use serde::{Deserialize, Serialize};

use common::conn::BoxFuture;
use database::DatabaseClient;

use app::common::{NetAddress, PrivateKey, PublicKey};
use node::{NodeMutation, NodeState};

use identity::IdentityClient;

use crate::compact_node::CompactState;
use crate::messages::{NodeInfo, NodeName};

#[derive(Debug, Clone)]
pub struct LoadedNodeLocal {
    pub node_identity_client: IdentityClient,
    pub compact_state: CompactState,
    pub compact_db_client: DatabaseClient<CompactState>,
    pub node_state: NodeState<NetAddress>,
    pub node_db_client: DatabaseClient<NodeMutation<NetAddress>>,
}

#[derive(Debug, Clone)]
pub struct LoadedNodeRemote {
    pub app_identity_client: IdentityClient,
    pub node_public_key: PublicKey,
    pub node_address: NetAddress,
    pub compact_state: CompactState,
    pub compact_db_client: DatabaseClient<CompactState>,
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone)]
pub enum LoadedNode {
    Local(LoadedNodeLocal),
    Remote(LoadedNodeRemote),
}

#[allow(clippy::large_enum_variant)]
#[derive(Arbitrary, Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StoredNodeConfig {
    pub is_enabled: bool,
}

#[derive(Debug, Clone)]
pub struct StoredNode {
    pub info: NodeInfo,
    pub config: StoredNodeConfig,
}

pub type StoredNodes = HashMap<NodeName, StoredNode>;

pub trait StoreError: Debug {
    fn is_fatal(&self) -> bool;
}

// TODO: Possibly implement encryption for nodes' private key here:
/// Persistent storage manager for nodes' private information.
pub trait Store {
    type Error: StoreError;

    fn create_local_node(
        &mut self,
        node_name: NodeName,
        node_private_key: PrivateKey,
    ) -> BoxFuture<'_, Result<(), Self::Error>>;

    fn create_remote_node(
        &mut self,
        node_name: NodeName,
        app_private_key: PrivateKey,
        node_public_key: PublicKey,
        node_address: NetAddress,
    ) -> BoxFuture<'_, Result<(), Self::Error>>;

    // TODO: Possibly become generic over NodeConfig in the future
    // (Add it as a type to the Store trait?)
    /// Set persistent configuration for a node
    fn config_node(
        &mut self,
        node_name: NodeName,
        node_config: StoredNodeConfig,
    ) -> BoxFuture<'_, Result<(), Self::Error>>;

    /// List all existing nodes in store
    fn list_nodes(&self) -> BoxFuture<'_, Result<StoredNodes, Self::Error>>;

    /// Load (private) information of one node
    fn load_node(&mut self, node_name: NodeName) -> BoxFuture<'_, Result<LoadedNode, Self::Error>>;

    /// Unload a node
    fn unload_node<'a>(
        &'a mut self,
        node_name: &'a NodeName,
    ) -> BoxFuture<'a, Result<(), Self::Error>>;

    /// Remove a node from the store
    /// A node must be in unloaded state to be removed.
    fn remove_node(&mut self, node_name: NodeName) -> BoxFuture<'_, Result<(), Self::Error>>;
}
