use std::collections::HashMap;
use std::hash::Hash;

use serde::{Deserialize, Serialize};

use app::common::{NetAddress, PrivateKey, PublicKey, Uid};
use app::conn::AppPermissions;

use crate::compact_node::{CompactReport, CompactToUser, UserToCompact};

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Hash, Clone)]
pub struct NodeName(String);

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Hash, Clone)]
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
pub struct NodeStatus {
    pub is_open: bool,
    pub info: NodeInfo,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct CreateNodeLocal {
    pub node_name: NodeName,
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
    Success(NodeName, NodeId, AppPermissions, CompactReport), // (node_name, node_id, compact_report)
    Failure(NodeName),
}

pub type NodesInfo = HashMap<NodeName, NodeInfo>;

pub type NodesStatus = HashMap<NodeName, NodeStatus>;

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum RequestCreateNode {
    CreateNodeLocal(CreateNodeLocal),
    CreateNodeRemote(CreateNodeRemote),
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum ServerToUser {
    ResponseOpenNode(ResponseOpenNode),
    /// A map of all nodes and their current status
    NodesStatus(NodesStatus),
    /// A message received from a specific node
    Node(NodeId, CompactToUser), // (node_id, compact_to_user)
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum ServerToUserAck {
    ServerToUser(ServerToUser),
    Ack(Uid),
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum UserToServer {
    RequestCreateNode(RequestCreateNode),
    RequestRemoveNode(NodeName),
    RequestOpenNode(NodeName),
    RequestCloseNode(NodeId), // node_id
    /// A message sent to a specific node
    Node(NodeId, UserToCompact), // (node_id, user_to_compact)
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct UserToServerAck {
    pub request_id: Uid,
    pub inner: UserToServer,
}
