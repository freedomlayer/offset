use crypto::identity::PublicKey;
use crate::net::messages::NetAddress;

/// Address of a node. Used by an application to connect to a node.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NodeAddress<B=NetAddress> {
    pub public_key: PublicKey,
    pub address: B,
}
