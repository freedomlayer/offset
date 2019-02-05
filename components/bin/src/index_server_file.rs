use std::io::{self, Write};
use std::fs::{self, File};
use std::path::PathBuf;

use std::net::SocketAddr;

use toml;
use base64::{self, URL_SAFE_NO_PAD};

use crypto::identity::{PublicKey, PUBLIC_KEY_LEN};
use net::{TcpListener, socket_addr_to_tcp_address};

use proto::index_server::messages::IndexServerAddress;

#[derive(Debug)]
pub enum IndexServerFileError {
    IoError(io::Error),
    TomlDeError(toml::de::Error),
    Base64DecodeError(base64::DecodeError),
    ParseSocketAddrError,
    InvalidPublicKey,
}

/// A helper structure for serialize and deserializing IndexServerAddress.
#[derive(Deserialize)]
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

impl From<base64::DecodeError> for IndexServerFileError {
    fn from(e: base64::DecodeError) -> Self {
        IndexServerFileError::Base64DecodeError(e)
    }
}

/// Load IndexServerAddress from a file
pub fn load_index_server_from_file(path_buf: PathBuf) -> Result<IndexServerAddress, IndexServerFileError> {
    let data = fs::read_to_string(path_buf)?;
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

pub fn store_index_server_to_file(index_server_address: &IndexServerAddress, path_buf: PathBuf) {
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_index_server_file_basic() {
        let index_server_file: IndexServerFile = toml::from_str(r#"
            public_key = 'public_key_string'
            address = 'address_string'
        "#).unwrap();

        assert_eq!(index_server_file.public_key, "public_key_string");
        assert_eq!(index_server_file.address, "address_string");
    }
}

