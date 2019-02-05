use std::io::{self, Write};
use std::fs::{self, File};
use std::path::Path;

use std::net::SocketAddr;

use toml;
use base64::{self, URL_SAFE_NO_PAD};

use crypto::identity::{PublicKey, PUBLIC_KEY_LEN};
use net::{TcpListener, socket_addr_to_tcp_address, 
    tcp_address_to_socket_addr};

use proto::index_server::messages::IndexServerAddress;

#[derive(Debug)]
pub enum IndexServerFileError {
    IoError(io::Error),
    TomlDeError(toml::de::Error),
    TomlSeError(toml::ser::Error),
    Base64DecodeError(base64::DecodeError),
    ParseSocketAddrError,
    InvalidPublicKey,
}

/// A helper structure for serialize and deserializing IndexServerAddress.
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

impl From<base64::DecodeError> for IndexServerFileError {
    fn from(e: base64::DecodeError) -> Self {
        IndexServerFileError::Base64DecodeError(e)
    }
}

/// Load IndexServerAddress from a file
pub fn load_index_server_from_file(path: &Path) -> Result<IndexServerAddress, IndexServerFileError> {
    let data = fs::read_to_string(&path)?;
    let index_server_file: IndexServerFile = toml::from_str(&data)?;

    // Decode public key:
    let public_key_vec = base64::decode_config(&index_server_file.public_key, URL_SAFE_NO_PAD)?;
    // TODO: A more idiomatic way to do this?
    if public_key_vec.len() != PUBLIC_KEY_LEN {
        return Err(IndexServerFileError::InvalidPublicKey);
    }
    let mut public_key_array = [0u8; PUBLIC_KEY_LEN];
    public_key_array.copy_from_slice(&public_key_vec[0 .. PUBLIC_KEY_LEN]);
    let public_key = PublicKey::from(&public_key_array);

    // Decode address:
    let socket_addr: SocketAddr = index_server_file.address.parse()
        .map_err(|_| IndexServerFileError::ParseSocketAddrError)?;
    let address = socket_addr_to_tcp_address(&socket_addr);

    Ok(IndexServerAddress {
        public_key,
        address,
    })
}


/// Store IndexServerAddress to file
pub fn store_index_server_to_file(index_server_address: &IndexServerAddress, path: &Path)
    -> Result<(), IndexServerFileError> {

    let IndexServerAddress {ref public_key, ref address} = index_server_address;

    let socket_addr = tcp_address_to_socket_addr(&address);
    let socket_addr_str = format!("{}", socket_addr);

    let index_server_file = IndexServerFile {
        public_key: base64::encode_config(&public_key, URL_SAFE_NO_PAD),
        address: socket_addr_str,
    };

    let data = toml::to_string(&index_server_file)?;

    let mut file = File::create(path)?;
    file.write(&data.as_bytes())?;

    Ok(())
}


/// Load a directory of index server address files, and return a map representing
/// the information from all files
pub fn load_trusted_servers(config_dir: &Path) -> Result<Vec<IndexServerAddress>, IndexServerFileError> {
    let mut res_trusted = Vec::new();
    for entry in fs::read_dir(config_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            continue;
        }
        res_trusted.push(load_index_server_from_file(&path)?);
    }
    Ok(res_trusted)
}



#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use proto::funder::messages::{TcpAddress, TcpAddressV4};

    #[test]
    fn test_index_server_file_basic() {
        let index_server_file: IndexServerFile = toml::from_str(r#"
            public_key = 'public_key_string'
            address = 'address_string'
        "#).unwrap();

        assert_eq!(index_server_file.public_key, "public_key_string");
        assert_eq!(index_server_file.address, "address_string");
    }

    #[test]
    fn test_store_load_index_server() {
        // Create a temporary directory:
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("index_server_address_file");

        let address = TcpAddress::V4(TcpAddressV4 {
            octets: [127,0,0,1],
            port: 1337,
        });

        let index_server_address = IndexServerAddress {
            public_key: PublicKey::from(&[0xaa; PUBLIC_KEY_LEN]),
            address,
        };

        store_index_server_to_file(&index_server_address, &file_path).unwrap();
        let index_server_address2 = load_index_server_from_file(&file_path).unwrap();

        assert_eq!(index_server_address, index_server_address2);
    }


    #[test]
    fn test_load_trusted_servers() {
        // Create a temporary directory:
        let dir = tempdir().unwrap();

        let file_path = dir.path().join("index_server_address_file_a");
        let address = TcpAddress::V4(TcpAddressV4 {
            octets: [127,0,0,1],
            port: 0xa,
        });
        let index_server_address = IndexServerAddress {
            public_key: PublicKey::from(&[0xaa; PUBLIC_KEY_LEN]),
            address,
        };
        store_index_server_to_file(&index_server_address, &file_path).unwrap();

        let file_path = dir.path().join("index_server_address_file_b");
        let address = TcpAddress::V4(TcpAddressV4 {
            octets: [127,0,0,1],
            port: 0xb,
        });
        let index_server_address = IndexServerAddress {
            public_key: PublicKey::from(&[0xbb; PUBLIC_KEY_LEN]),
            address,
        };
        store_index_server_to_file(&index_server_address, &file_path).unwrap();


        let file_path = dir.path().join("index_server_address_file_c");
        let address = TcpAddress::V4(TcpAddressV4 {
            octets: [127,0,0,1],
            port: 0xc,
        });
        let index_server_address = IndexServerAddress {
            public_key: PublicKey::from(&[0xcc; PUBLIC_KEY_LEN]),
            address,
        };
        store_index_server_to_file(&index_server_address, &file_path).unwrap();


        let trusted_servers = load_trusted_servers(&dir.path()).unwrap();
        assert_eq!(trusted_servers.len(), 3);

        let mut ports = Vec::new();
        for t_server in trusted_servers {
            let port = match t_server.address {
                TcpAddress::V4(tcp_address_v4) => tcp_address_v4.port,
                _ => unreachable!(),
            };
            ports.push(port);
        }
        ports.sort();
        assert_eq!(ports, vec![0xa,0xb,0xc]);
    }
}

