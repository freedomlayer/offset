use serde::{Deserialize, Serialize};

use app::common::{NetAddress, PrivateKey, PublicKey};

use crate::compact_node::{CompactReport, FromUser, ToUser};

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct NodeName(String);

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
pub enum NodeInfo {
    Local(NodeName),                         // node_name
    Remote(NodeName, PublicKey, NetAddress), // (node_name, node_public_key, node_address)
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum CreateNodeResult {
    Success,
    Failure,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct CreateNodeLocal {
    pub name: NodeName,
    pub node_private_key: PrivateKey,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct CreateNodeRemote {
    pub name: NodeName,
    pub app_private_key: PrivateKey,
    pub node_public_key: PublicKey,
    pub node_address: NetAddress,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum ResponseOpenNode {
    Success(NodeName, CompactReport),
    Failure(NodeName),
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct NodeList {
    node_list: Vec<NodeInfo>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum RequestCreateNode {
    CreateNodeLocal(CreateNodeLocal),
    CreateNodeRemote(CreateNodeRemote),
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum ServerToUser {
    ResponseOpenNode(ResponseOpenNode),
    ResponseCreateNode(NodeName, CreateNodeResult),
    ResponseRemoveNode(NodeName),
    ResponseCloseNode(NodeName),
    /// A message received from a specific node
    Node(NodeName, ToUser),
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum UserToServer {
    RequestCreateNode(RequestCreateNode),
    RequestRemoveNode(NodeName),
    RequestOpenNode(NodeName),
    RequestCloseNode(NodeName),
    /// A message sent to a specific node
    Node(NodeName, FromUser),
}
