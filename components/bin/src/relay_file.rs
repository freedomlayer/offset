use std::io::{self, Write};
use std::fs::{self, File};
use std::path::Path;

use std::net::SocketAddr;

use toml;
use base64::{self, URL_SAFE_NO_PAD};

use crypto::identity::{PublicKey, PUBLIC_KEY_LEN};
use net::{socket_addr_to_tcp_address, tcp_address_to_socket_addr};

use proto::funder::messages::RelayAddress;

#[derive(Debug)]
pub enum RelayFileError {
    IoError(io::Error),
    TomlDeError(toml::de::Error),
    TomlSeError(toml::ser::Error),
    Base64DecodeError(base64::DecodeError),
    ParseSocketAddrError,
    InvalidPublicKey,
}

/// A helper structure for serialize and deserializing RelayAddress.
#[derive(Serialize, Deserialize)]
struct RelayFile {
    public_key: String,
    address: String,
}

impl From<io::Error> for RelayFileError {
    fn from(e: io::Error) -> Self {
        RelayFileError::IoError(e)
    }
}

impl From<toml::de::Error> for RelayFileError {
    fn from(e: toml::de::Error) -> Self {
        RelayFileError::TomlDeError(e)
    }
}

impl From<toml::ser::Error> for RelayFileError {
    fn from(e: toml::ser::Error) -> Self {
        RelayFileError::TomlSeError(e)
    }
}

impl From<base64::DecodeError> for RelayFileError {
    fn from(e: base64::DecodeError) -> Self {
        RelayFileError::Base64DecodeError(e)
    }
}

/// Load RelayAddress from a file
#[allow(unused)]
pub fn load_relay_from_file(path: &Path) -> Result<RelayAddress, RelayFileError> {
    let data = fs::read_to_string(&path)?;
    let relay_file: RelayFile = toml::from_str(&data)?;

    // Decode public key:
    let public_key_vec = base64::decode_config(&relay_file.public_key, URL_SAFE_NO_PAD)?;
    // TODO: A more idiomatic way to do this?
    if public_key_vec.len() != PUBLIC_KEY_LEN {
        return Err(RelayFileError::InvalidPublicKey);
    }
    let mut public_key_array = [0u8; PUBLIC_KEY_LEN];
    public_key_array.copy_from_slice(&public_key_vec[0 .. PUBLIC_KEY_LEN]);
    let public_key = PublicKey::from(&public_key_array);

    // Decode address:
    let socket_addr: SocketAddr = relay_file.address.parse()
        .map_err(|_| RelayFileError::ParseSocketAddrError)?;
    let address = socket_addr_to_tcp_address(&socket_addr);

    Ok(RelayAddress {
        public_key,
        address,
    })
}


/// Store RelayAddress to file
pub fn store_relay_to_file(relay_address: &RelayAddress, path: &Path)
    -> Result<(), RelayFileError> {

    let RelayAddress {ref public_key, ref address} = relay_address;

    let socket_addr = tcp_address_to_socket_addr(&address);
    let socket_addr_str = format!("{}", socket_addr);

    let relay_file = RelayFile {
        public_key: base64::encode_config(&public_key, URL_SAFE_NO_PAD),
        address: socket_addr_str,
    };

    let data = toml::to_string(&relay_file)?;

    let mut file = File::create(path)?;
    file.write(&data.as_bytes())?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use proto::net::messages::{TcpAddress, TcpAddressV4};

    #[test]
    fn test_relay_file_basic() {
        let relay_file: RelayFile = toml::from_str(r#"
            public_key = 'public_key_string'
            address = 'address_string'
        "#).unwrap();

        assert_eq!(relay_file.public_key, "public_key_string");
        assert_eq!(relay_file.address, "address_string");
    }

    #[test]
    fn test_store_load_relay() {
        // Create a temporary directory:
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("relay_address_file");

        let address = TcpAddress::V4(TcpAddressV4 {
            octets: [127,0,0,1],
            port: 1337,
        });

        let relay_address = RelayAddress {
            public_key: PublicKey::from(&[0xaa; PUBLIC_KEY_LEN]),
            address,
        };

        store_relay_to_file(&relay_address, &file_path).unwrap();
        let relay_address2 = load_relay_from_file(&file_path).unwrap();

        assert_eq!(relay_address, relay_address2);
    }
}


