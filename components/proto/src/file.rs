use crypto::identity::{PrivateKey, PublicKey};

use crate::ser_string::{from_base64, to_base64};

use crate::app_server::messages::AppPermissions;
use crate::app_server::messages::RelayAddress;
use crate::net::messages::NetAddress;
// use crate::node::types::NodeAddress;
// use crate::file::relay::RelayFile;

/// A helper structure for serialize and deserializing IndexServerAddress.
#[derive(Debug, Serialize, Deserialize)]
pub struct TrustedAppFile {
    #[serde(serialize_with = "to_base64", deserialize_with = "from_base64")]
    pub public_key: PublicKey,
    pub permissions: AppPermissions,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FriendAddressFile {
    pub public_key: PublicKey,
    pub relays: Vec<RelayAddress>,
}

/// A helper structure for serialize and deserializing RelayAddress.
#[derive(Serialize, Deserialize)]
pub struct RelayFile {
    #[serde(serialize_with = "to_base64", deserialize_with = "from_base64")]
    pub public_key: PublicKey,
    pub address: NetAddress,
}

/// A helper structure for serialize and deserializing FriendAddress.
#[derive(Serialize, Deserialize)]
pub struct FriendFile {
    #[serde(serialize_with = "to_base64", deserialize_with = "from_base64")]
    pub public_key: PublicKey,
    pub relays: Vec<RelayFile>,
}

/// A helper structure for serialize and deserializing IdentityAddress.
#[derive(Serialize, Deserialize)]
pub struct IdentityFile {
    #[serde(serialize_with = "to_base64", deserialize_with = "from_base64")]
    pub private_key: PrivateKey,
}

/// A helper structure for serialize and deserializing IndexServer.
#[derive(Serialize, Deserialize)]
pub struct IndexServerFile {
    #[serde(serialize_with = "to_base64", deserialize_with = "from_base64")]
    pub public_key: PublicKey,
    pub address: NetAddress,
}

/// A helper structure for serialize and deserializing NodeAddress.
#[derive(Serialize, Deserialize)]
pub struct NodeAddressFile {
    #[serde(serialize_with = "to_base64", deserialize_with = "from_base64")]
    pub public_key: PublicKey,
    pub address: NetAddress,
}

/*

// TODO: Turn this construct to be a macro (procedural?)
impl From<NodeFile> for NodeAddress {
    fn from(node_file: NodeFile) -> Self {
        NodeAddress {
            public_key: node_file.public_key,
            address: node_file.address,
        }
    }
}

impl From<NodeAddress> for NodeFile {
    fn from(node_address: NodeAddress) -> Self {
        NodeFile {
            public_key: node_address.public_key,
            address: node_address.address,
        }
    }
}
*/
