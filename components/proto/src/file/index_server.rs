use std::convert::TryInto;
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use toml;

use crate::file::ser_string::{public_key_to_string, string_to_public_key, SerStringError};

use crate::index_server::messages::IndexServerAddress;
use crate::net::messages::{NetAddress, NetAddressError};

#[derive(Debug)]
pub enum IndexServerFileError {
    IoError(io::Error),
    TomlDeError(toml::de::Error),
    TomlSeError(toml::ser::Error),
    SerStringError,
    ParseSocketAddrError,
    InvalidPublicKey,
    NetAddressError(NetAddressError),
}

/// A helper structure for serialize and deserializing IndexServer.
#[derive(Serialize, Deserialize)]
struct IndexServerFile {
    public_key: String,
    address: String,
}

impl From<io::Error> for IndexServerFileError {
    fn from(e: io::Error) -> Self {
        IndexServerFileError::IoError(e)
    }
}

impl From<toml::de::Error> for IndexServerFileError {
    fn from(e: toml::de::Error) -> Self {
        IndexServerFileError::TomlDeError(e)
    }
}

impl From<toml::ser::Error> for IndexServerFileError {
    fn from(e: toml::ser::Error) -> Self {
        IndexServerFileError::TomlSeError(e)
    }
}

impl From<SerStringError> for IndexServerFileError {
    fn from(_e: SerStringError) -> Self {
        IndexServerFileError::SerStringError
    }
}

impl From<NetAddressError> for IndexServerFileError {
    fn from(e: NetAddressError) -> Self {
        IndexServerFileError::NetAddressError(e)
    }
}

/// Load IndexServer from a file
pub fn load_index_server_from_file(
    path: &Path,
) -> Result<IndexServerAddress<NetAddress>, IndexServerFileError> {
    let data = fs::read_to_string(&path)?;
    let index_server_file: IndexServerFile = toml::from_str(&data)?;

    let public_key = string_to_public_key(&index_server_file.public_key)?;

    Ok(IndexServerAddress {
        public_key,
        address: index_server_file.address.try_into()?,
    })
}

/// Store IndexServer to file
pub fn store_index_server_to_file(
    index_server: &IndexServerAddress<NetAddress>,
    path: &Path,
) -> Result<(), IndexServerFileError> {
    let IndexServerAddress {
        ref public_key,
        ref address,
    } = index_server;

    let index_server_file = IndexServerFile {
        public_key: public_key_to_string(&public_key),
        address: address.as_str().to_string(),
    };

    let data = toml::to_string(&index_server_file)?;

    let mut file = File::create(path)?;
    file.write_all(&data.as_bytes())?;

    Ok(())
}

#[derive(Debug)]
pub enum IndexServerDirectoryError {
    IoError(io::Error),
    InvalidDirectory(io::Error),
    InvalidFile(PathBuf, IndexServerFileError),
}

impl From<io::Error> for IndexServerDirectoryError {
    fn from(e: io::Error) -> Self {
        IndexServerDirectoryError::IoError(e)
    }
}

/// Load a directory of index server address files, and return a map representing
/// the information from all files
pub fn load_trusted_servers(
    dir_path: &Path,
) -> Result<Vec<IndexServerAddress<NetAddress>>, IndexServerDirectoryError> {
    let mut res_trusted = Vec::new();
    for entry in fs::read_dir(dir_path).map_err(IndexServerDirectoryError::InvalidDirectory)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            continue;
        }
        res_trusted.push(
            load_index_server_from_file(&path)
                .map_err(|e| IndexServerDirectoryError::InvalidFile(path, e))?,
        );
    }
    Ok(res_trusted)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    use crypto::identity::{PublicKey, PUBLIC_KEY_LEN};

    #[test]
    fn test_index_server_file_basic() {
        let index_server_file: IndexServerFile = toml::from_str(
            r#"
            public_key = 'public_key_string'
            address = 'localhost:1337'
        "#,
        )
        .unwrap();

        assert_eq!(index_server_file.public_key, "public_key_string");
        assert_eq!(index_server_file.address, "localhost:1337");
    }

    #[test]
    fn test_store_load_index_server() {
        // Create a temporary directory:
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("index_server_address_file");

        let index_server_address = IndexServerAddress {
            public_key: PublicKey::from(&[0xaa; PUBLIC_KEY_LEN]),
            address: "127.0.0.1:1337".to_owned().try_into().unwrap(),
        };

        store_index_server_to_file(&index_server_address, &file_path).unwrap();
        let index_server_address2 = load_index_server_from_file(&file_path).unwrap();

        assert_eq!(index_server_address, index_server_address2);
    }

    #[test]
    fn test_load_trusted_index_servers() {
        // Create a temporary directory:
        let dir = tempdir().unwrap();

        let file_path = dir.path().join("index_server_address_file_a");
        let index_server_address = IndexServerAddress {
            public_key: PublicKey::from(&[0xaa; PUBLIC_KEY_LEN]),
            address: "127.0.0.1:1000".to_owned().try_into().unwrap(),
        };
        store_index_server_to_file(&index_server_address, &file_path).unwrap();

        let file_path = dir.path().join("index_server_address_file_b");
        let index_server_address = IndexServerAddress {
            public_key: PublicKey::from(&[0xbb; PUBLIC_KEY_LEN]),
            address: "127.0.0.1:1001".to_owned().try_into().unwrap(),
        };
        store_index_server_to_file(&index_server_address, &file_path).unwrap();

        let file_path = dir.path().join("index_server_address_file_c");
        let index_server_address = IndexServerAddress {
            public_key: PublicKey::from(&[0xcc; PUBLIC_KEY_LEN]),
            address: "127.0.0.1:1002".to_owned().try_into().unwrap(),
        };
        store_index_server_to_file(&index_server_address, &file_path).unwrap();

        let trusted_servers = load_trusted_servers(&dir.path()).unwrap();
        assert_eq!(trusted_servers.len(), 3);

        let mut addresses = trusted_servers
            .iter()
            .map(|server| server.address.clone())
            .collect::<Vec<_>>();
        addresses.sort();

        assert_eq!(
            addresses,
            vec![
                "127.0.0.1:1000".to_owned().try_into().unwrap(),
                "127.0.0.1:1001".to_owned().try_into().unwrap(),
                "127.0.0.1:1002".to_owned().try_into().unwrap()
            ]
        );
    }
}
