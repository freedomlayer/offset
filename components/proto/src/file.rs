use serde::{Deserialize, Serialize};

use crate::crypto::{PrivateKey, PublicKey};

use mutual_from::mutual_from;

use common::ser_utils::{SerBase64, SerString};

use crate::app_server::messages::{AppPermissions, RelayAddress};
use crate::net::messages::NetAddress;

/// A helper structure for serialize and deserializing IndexServerAddress.
#[derive(Debug, Serialize, Deserialize)]
pub struct TrustedAppFile {
    #[serde(with = "SerBase64")]
    pub public_key: PublicKey,
    pub permissions: AppPermissions,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct FriendAddressFile {
    #[serde(with = "SerBase64")]
    pub public_key: PublicKey,
    pub relays: Vec<RelayAddressFile>,
}

/// A helper structure for serialize and deserializing RelayAddress.
#[mutual_from(RelayAddress)]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RelayAddressFile {
    #[serde(with = "SerBase64")]
    pub public_key: PublicKey,
    #[serde(with = "SerString")]
    pub address: NetAddress,
}

/// A helper structure for serialize and deserializing FriendAddress.
#[derive(Debug, Serialize, Deserialize)]
pub struct FriendFile {
    #[serde(with = "SerBase64")]
    pub public_key: PublicKey,
    pub relays: Vec<RelayAddressFile>,
}

/// A helper structure for serialize and deserializing IdentityAddress.
#[derive(Debug, Serialize, Deserialize)]
pub struct IdentityFile {
    #[serde(with = "SerBase64")]
    pub private_key: PrivateKey,
}

/// A helper structure for serialize and deserializing IndexServer.
#[derive(Debug, Serialize, Deserialize)]
pub struct IndexServerFile {
    #[serde(with = "SerBase64")]
    pub public_key: PublicKey,
    pub address: NetAddress,
}

/// A helper structure for serialize and deserializing NodeAddress.
#[derive(Debug, Serialize, Deserialize)]
pub struct NodeAddressFile {
    #[serde(with = "SerBase64")]
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
