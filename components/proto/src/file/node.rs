// use std::fs::{self, File};
// use std::io::{self, Write};
use std::path::Path;

use crypto::identity::PublicKey;

use crate::file::ser_string::{from_base64, load_from_file, store_to_file, to_base64, FileError};
// use toml;

use crate::net::messages::NetAddress;
use crate::node::types::NodeAddress;

/*
#[derive(Debug, From)]
pub enum NodeFileError {
    IoError(io::Error),
    TomlDeError(toml::de::Error),
    TomlSeError(toml::ser::Error),
    ParseSocketAddrError,
    InvalidPublicKey,
    NetAddressError(NetAddressError),
}
*/

/// A helper structure for serialize and deserializing NodeAddress.
#[derive(Serialize, Deserialize)]
struct NodeFile {
    #[serde(serialize_with = "to_base64", deserialize_with = "from_base64")]
    public_key: PublicKey,
    address: NetAddress,
}

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

/// Load NodeAddress from a file
pub fn load_node_from_file(path: &Path) -> Result<NodeAddress, FileError> {
    load_from_file::<NodeFile, _>(path)
    /*
    let data = fs::read_to_string(&path)?;
    let node_file: NodeFile = toml::from_str(&data)?;

    Ok(NodeAddress {
        public_key: node_file.public_key,
        address: node_file.address,
    })
    */
}

/// Store NodeAddress to file
pub fn store_node_to_file(node_address: &NodeAddress, path: &Path) -> Result<(), FileError> {
    store_to_file::<NodeFile, _>(node_address.clone(), path)

    /*
    let NodeAddress {
        ref public_key,
        ref address,
    } = node_address;

    let node_file = NodeFile {
        public_key: public_key.clone(),
        address: address.clone(),
    };

    let data = toml::to_string(&node_file)?;

    let mut file = File::create(path)?;
    file.write_all(&data.as_bytes())?;

    Ok(())
    */
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::convert::TryInto;

    use tempfile::tempdir;

    use crypto::identity::{PublicKey, PUBLIC_KEY_LEN};

    /*
    #[test]
    fn test_node_file_basic() {
        let node_file: NodeFile = toml::from_str(
            r#"
            public_key = 'public_key_string'
            address = 'address_string'
        "#,
        )
        .unwrap();

        assert_eq!(node_file.public_key, "public_key_string");
        assert_eq!(node_file.address, "address_string");
    }
    */

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
