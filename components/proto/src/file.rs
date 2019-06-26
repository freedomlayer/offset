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

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct FriendAddressFile {
    #[serde(serialize_with = "to_base64", deserialize_with = "from_base64")]
    pub public_key: PublicKey,
    pub relays: Vec<RelayAddress>,
}

/// A helper structure for serialize and deserializing RelayAddress.
#[derive(Debug, Serialize, Deserialize)]
pub struct RelayAddressFile {
    #[serde(serialize_with = "to_base64", deserialize_with = "from_base64")]
    pub public_key: PublicKey,
    pub address: NetAddress,
}

/// A helper structure for serialize and deserializing FriendAddress.
#[derive(Debug, Serialize, Deserialize)]
pub struct FriendFile {
    #[serde(serialize_with = "to_base64", deserialize_with = "from_base64")]
    pub public_key: PublicKey,
    pub relays: Vec<RelayAddressFile>,
}

/// A helper structure for serialize and deserializing IdentityAddress.
#[derive(Debug, Serialize, Deserialize)]
pub struct IdentityFile {
    #[serde(serialize_with = "to_base64", deserialize_with = "from_base64")]
    pub private_key: PrivateKey,
}

/// A helper structure for serialize and deserializing IndexServer.
#[derive(Debug, Serialize, Deserialize)]
pub struct IndexServerFile {
    #[serde(serialize_with = "to_base64", deserialize_with = "from_base64")]
    pub public_key: PublicKey,
    pub address: NetAddress,
}

/// A helper structure for serialize and deserializing NodeAddress.
#[derive(Debug, Serialize, Deserialize)]
pub struct NodeAddressFile {
    #[serde(serialize_with = "to_base64", deserialize_with = "from_base64")]
    pub public_key: PublicKey,
    pub address: NetAddress,
}

// TODO: Possibly create a macro that these conversions:
impl std::convert::From<RelayAddressFile> for RelayAddress {
    fn from(input: RelayAddressFile) -> Self {
        RelayAddress {
            public_key: input.public_key,
            address: input.address,
        }
    }
}

impl std::convert::From<RelayAddress> for RelayAddressFile {
    fn from(input: RelayAddress) -> Self {
        RelayAddressFile {
            public_key: input.public_key,
            address: input.address,
        }
    }
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
