use std::collections::HashMap;
use std::hash::Hash;

use serde::{Deserialize, Serialize};

use common::ser_utils::{ser_b64, ser_string};

use app::common::{NetAddress, PrivateKey, PublicKey, Uid};
use app::conn::AppPermissions;

use crate::compact_node::messages::{CompactReport, CompactToUser, UserToCompact};

#[derive(Arbitrary, Debug, Serialize, Deserialize, PartialEq, Eq, Hash, Clone)]
#[serde(rename_all = "camelCase")]
pub struct NodeName(String);

#[derive(Arbitrary, Debug, Serialize, Deserialize, PartialEq, Eq, Hash, Clone)]
#[serde(rename_all = "camelCase")]
pub struct NodeId(#[serde(with = "ser_string")] pub u64);

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

#[derive(Arbitrary, Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct NodeInfoLocal {
    #[serde(with = "ser_b64")]
    pub node_public_key: PublicKey,
}

#[derive(Arbitrary, Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct NodeInfoRemote {
    #[serde(with = "ser_b64")]
    pub app_public_key: PublicKey,
    #[serde(with = "ser_b64")]
    pub node_public_key: PublicKey,
    pub node_address: NetAddress,
}

#[derive(Arbitrary, Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum NodeInfo {
    Local(NodeInfoLocal),
    Remote(NodeInfoRemote),
}

#[derive(Arbitrary, Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct NodeConfig {
    pub is_enabled: bool,
}

#[derive(Arbitrary, Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StoredNode {
    pub info: NodeInfo,
    pub config: NodeConfig,
}

#[derive(Arbitrary, Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct NodeStatus {
    pub is_open: bool,
    pub is_enabled: bool,
    pub info: NodeInfo,
}

#[derive(Arbitrary, Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CreateNodeLocal {
    pub node_name: NodeName,
}

#[derive(Arbitrary, Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CreateNodeRemote {
    pub node_name: NodeName,
    #[serde(with = "ser_b64")]
    pub app_private_key: PrivateKey,
    #[serde(with = "ser_b64")]
    pub node_public_key: PublicKey,
    pub node_address: NetAddress,
}

#[allow(clippy::large_enum_variant)]
#[derive(Arbitrary, Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ResponseOpenNode {
    Success(NodeName, NodeId, AppPermissions, CompactReport), // (node_name, node_id, compact_report)
    Failure(NodeName),
}

pub type NodesStatus = HashMap<NodeName, NodeStatus>;

#[derive(Arbitrary, Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum CreateNode {
    CreateNodeLocal(CreateNodeLocal),
    CreateNodeRemote(CreateNodeRemote),
}

#[allow(clippy::large_enum_variant)]
#[derive(Arbitrary, Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ServerToUser {
    ResponseOpenNode(ResponseOpenNode),
    // TODO: Should add a serde hint here?
    /// A map of all nodes and their current status
    NodesStatus(NodesStatus),
    /// A message received from a specific node
    Node(NodeId, CompactToUser), // (node_id, compact_to_user)
}

#[allow(clippy::large_enum_variant)]
#[derive(Arbitrary, Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ServerToUserAck {
    ServerToUser(ServerToUser),
    #[serde(with = "ser_b64")]
    Ack(Uid),
}

#[allow(clippy::large_enum_variant)]
#[derive(Arbitrary, Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum UserToServer {
    CreateNode(CreateNode),
    RemoveNode(NodeName),
    RequestOpenNode(NodeName),
    CloseNode(NodeId), // node_id
    /// A message sent to a specific node
    Node(NodeId, UserToCompact), // (node_id, user_to_compact)
}

#[derive(Arbitrary, Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UserToServerAck {
    #[serde(with = "ser_b64")]
    pub request_id: Uid,
    pub inner: UserToServer,
}
