use serde::{Deserialize, Serialize};

use crate::crypto::{PrivateKey, PublicKey};

use mutual_from::mutual_from;

use common::ser_utils::{ser_b64, ser_string};

use crate::app_server::messages::{AppPermissions, RelayAddress};
use crate::net::messages::NetAddress;

/// A helper structure for serialize and deserializing IndexServerAddress.
#[derive(Arbitrary, Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TrustedAppFile {
    #[serde(with = "ser_b64")]
    pub public_key: PublicKey,
    pub permissions: AppPermissions,
}

#[derive(Arbitrary, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FriendAddressFile {
    #[serde(with = "ser_b64")]
    pub public_key: PublicKey,
    pub relays: Vec<RelayAddressFile>,
}

/// A helper structure for serialize and deserializing RelayAddress.
#[mutual_from(RelayAddress)]
#[derive(Arbitrary, Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RelayAddressFile {
    #[serde(with = "ser_b64")]
    pub public_key: PublicKey,
    #[serde(with = "ser_string")]
    pub address: NetAddress,
}

/// A helper structure for serialize and deserializing FriendAddress.
#[derive(Arbitrary, Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FriendFile {
    #[serde(with = "ser_b64")]
    pub public_key: PublicKey,
    pub relays: Vec<RelayAddressFile>,
}

/// A helper structure for serialize and deserializing IdentityAddress.
#[derive(Arbitrary, Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct IdentityFile {
    #[serde(with = "ser_b64")]
    pub private_key: PrivateKey,
}

/// A helper structure for serialize and deserializing IndexServer.
#[derive(Arbitrary, Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct IndexServerFile {
    #[serde(with = "ser_b64")]
    pub public_key: PublicKey,
    pub address: NetAddress,
}

/// A helper structure for serialize and deserializing NodeAddress.
#[derive(Arbitrary, Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct NodeAddressFile {
    #[serde(with = "ser_b64")]
    pub public_key: PublicKey,
    pub address: NetAddress,
}

/// A file with information used to connect to a remote node.
#[derive(Arbitrary, Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct NodeEntryFile {
    #[serde(with = "ser_b64")]
    pub node_public_key: PublicKey,
    pub node_address: NetAddress,
    #[serde(with = "ser_b64")]
    pub app_private_key: PrivateKey,
}
