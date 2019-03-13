use std::convert::TryInto;
use std::io::{self, Write};
use std::fs::{self, File};
use std::path::Path;

use toml;
use crate::file::pk_string::{public_key_to_string, 
    string_to_public_key, PkStringError};

use crate::net::messages::NetAddressError;
use crate::node::types::NodeAddress;

#[derive(Debug)]
pub enum NodeFileError {
    IoError(io::Error),
    TomlDeError(toml::de::Error),
    TomlSeError(toml::ser::Error),
    PkStringError,
    ParseSocketAddrError,
    InvalidPublicKey,
    NetAddressError(NetAddressError),
}

/// A helper structure for serialize and deserializing NodeAddress.
#[derive(Serialize, Deserialize)]
struct NodeFile {
    public_key: String,
    address: String,
}

impl From<io::Error> for NodeFileError {
    fn from(e: io::Error) -> Self {
        NodeFileError::IoError(e)
    }
}

impl From<toml::de::Error> for NodeFileError {
    fn from(e: toml::de::Error) -> Self {
        NodeFileError::TomlDeError(e)
    }
}

impl From<toml::ser::Error> for NodeFileError {
    fn from(e: toml::ser::Error) -> Self {
        NodeFileError::TomlSeError(e)
    }
}

impl From<PkStringError> for NodeFileError {
    fn from(_e: PkStringError) -> Self {
        NodeFileError::PkStringError
    }
}

impl From<NetAddressError> for NodeFileError {
    fn from(e: NetAddressError) -> Self {
        NodeFileError::NetAddressError(e)
    }
}

/// Load NodeAddress from a file
#[allow(unused)]
pub fn load_node_from_file(path: &Path) -> Result<NodeAddress, NodeFileError> {
    let data = fs::read_to_string(&path)?;
    let node_file: NodeFile = toml::from_str(&data)?;

    // Decode public key:
    let public_key = string_to_public_key(&node_file.public_key)?;

    Ok(NodeAddress {
        public_key,
        address: node_file.address.try_into()?,
    })
}


/// Store NodeAddress to file
pub fn store_node_to_file(node_address: &NodeAddress, path: &Path)
    -> Result<(), NodeFileError> {

    let NodeAddress {ref public_key, ref address} = node_address;

    let node_file = NodeFile {
        public_key: public_key_to_string(&public_key),
        address: address.as_str().to_string(),
    };

    let data = toml::to_string(&node_file)?;

    let mut file = File::create(path)?;
    file.write(&data.as_bytes())?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    use crypto::identity::{PublicKey, PUBLIC_KEY_LEN};

    #[test]
    fn test_node_file_basic() {
        let node_file: NodeFile = toml::from_str(r#"
            public_key = 'public_key_string'
            address = 'address_string'
        "#).unwrap();

        assert_eq!(node_file.public_key, "public_key_string");
        assert_eq!(node_file.address, "address_string");
    }

    #[test]
    fn test_store_load_node() {
        // Create a temporary directory:
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("node_address_file");

        let node_address = NodeAddress {
            public_key: PublicKey::from(&[0xaa; PUBLIC_KEY_LEN]),
            address: "127.0.0.1:1337".to_owned().try_into().unwrap(),
        };

        store_node_to_file(&node_address, &file_path).unwrap();
        let node_address2 = load_node_from_file(&file_path).unwrap();

        assert_eq!(node_address, node_address2);
    }
}



