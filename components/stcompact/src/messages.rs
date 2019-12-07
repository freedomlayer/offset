use std::collections::HashMap;
use std::hash::Hash;

use serde::{Deserialize, Serialize};

use app::common::{NetAddress, PrivateKey, PublicKey};

use crate::compact_node::{CompactReport, FromUser, ToUser};

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Hash, Clone)]
pub struct NodeName(String);

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct NodeId(pub u64);

impl NodeName {
    #[allow(unused)]
    pub fn new(node_name: String) -> Self {
        Self(node_name)
    }
    #[allow(unused)]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct NodeInfoLocal {
    pub node_public_key: PublicKey,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct NodeInfoRemote {
    pub app_public_key: PublicKey,
    pub node_public_key: PublicKey,
    pub node_address: NetAddress,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum NodeInfo {
    Local(NodeInfoLocal),
    Remote(NodeInfoRemote),
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum CreateNodeResult {
    Success(NodeInfo),
    Failure,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct CreateNodeLocal {
    pub node_name: NodeName,
    // pub node_private_key: PrivateKey,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct CreateNodeRemote {
    pub node_name: NodeName,
    pub app_private_key: PrivateKey,
    pub node_public_key: PublicKey,
    pub node_address: NetAddress,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum ResponseOpenNode {
    Success(NodeName, NodeId, CompactReport), // (node_name, node_id, compact_report)
    Failure(NodeName),
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct NodesInfo(pub HashMap<NodeName, NodeInfo>);

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum RequestCreateNode {
    CreateNodeLocal(CreateNodeLocal),
    CreateNodeRemote(CreateNodeRemote),
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum ServerToUser {
    // TODO: Possibly have (NodeName, ...) here: (???)
    ResponseOpenNode(ResponseOpenNode),
    ResponseCreateNode(NodeName, CreateNodeResult),
    // TODO: Should we include success/failure here: (???)
    ResponseRemoveNode(NodeName),
    // TODO: Should we include success/failure here: (???)
    ResponseCloseNode(NodeId), // node_id
    /// A message received from a specific node
    Node(NodeId, ToUser), // (node_id, to_user)
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum UserToServer {
    RequestCreateNode(RequestCreateNode),
    RequestRemoveNode(NodeName),
    RequestOpenNode(NodeName),
    RequestCloseNode(NodeId), // node_id
    /// A message sent to a specific node
    Node(NodeId, FromUser), // (node_id, to_user)
}
