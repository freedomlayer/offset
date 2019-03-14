use std::convert::TryInto;
use std::io::{self, Write};
use std::fs::{self, File};
use std::path::Path;

use toml;

use crypto::identity::PublicKey;
use net::messages::NetAddressError;

use crate::file::pk_string::{public_key_to_string, 
    string_to_public_key, PkStringError};

use crate::app_server::messages::RelayAddress;
use crate::file::relay::RelayFile;


#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FriendAddress {
    pub public_key: PublicKey,
    pub relays: Vec<RelayAddress>,
}

#[derive(Debug)]
pub enum FriendFileError {
    IoError(io::Error),
    TomlDeError(toml::de::Error),
    TomlSeError(toml::ser::Error),
    PkStringError,
    ParseSocketAddrError,
    InvalidPublicKey,
    NetAddressError(NetAddressError),
}

/// A helper structure for serialize and deserializing FriendAddress.
#[derive(Serialize, Deserialize)]
struct FriendFile {
    public_key: String,
    relays: Vec<RelayFile>,
}

impl From<io::Error> for FriendFileError {
    fn from(e: io::Error) -> Self {
        FriendFileError::IoError(e)
    }
}

impl From<toml::de::Error> for FriendFileError {
    fn from(e: toml::de::Error) -> Self {
        FriendFileError::TomlDeError(e)
    }
}

impl From<toml::ser::Error> for FriendFileError {
    fn from(e: toml::ser::Error) -> Self {
        FriendFileError::TomlSeError(e)
    }
}

impl From<PkStringError> for FriendFileError {
    fn from(_e: PkStringError) -> Self {
        FriendFileError::PkStringError
    }
}

impl From<NetAddressError> for FriendFileError {
    fn from(e: NetAddressError) -> Self {
        FriendFileError::NetAddressError(e)
    }
}

/// Load FriendAddress from a file
#[allow(unused)]
pub fn load_friend_from_file(path: &Path) -> Result<FriendAddress, FriendFileError> {
    let data = fs::read_to_string(&path)?;
    let friend_file: FriendFile = toml::from_str(&data)?;

    // Decode public key:
    let public_key = string_to_public_key(&friend_file.public_key)?;

    let mut relays = Vec::new();
    for relay_file in friend_file.relays {
        // Decode public key:
        let public_key = string_to_public_key(&relay_file.public_key)?;

        relays.push(RelayAddress {
            public_key,
            address: relay_file.address.try_into()?,
        });
    }

    Ok(FriendAddress {
        public_key,
        relays,
    })
}


/// Store FriendAddress to file
pub fn store_friend_to_file(friend_address: &FriendAddress, path: &Path)
    -> Result<(), FriendFileError> {

    let FriendAddress {ref public_key, ref relays} = friend_address;

    let mut relay_files = Vec::new();

    for relay_address in relays {
        let RelayAddress {ref public_key, ref address} = relay_address;

        relay_files.push(RelayFile {
            public_key: public_key_to_string(&public_key),
            address: address.as_str().to_string(),
        });
    }

    let friend_file = FriendFile {
        public_key: public_key_to_string(&public_key),
        relays: relay_files,
    };

    let data = toml::to_string(&friend_file)?;

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
    fn test_friend_file_basic() {
        let friend_file: FriendFile = toml::from_str(r#"
            public_key = 'qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqo'

            [[relays]]
            public_key = 'qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqo'
            address = '127.0.0.1:1337'

            [[relays]]
            public_key = 'u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7s'
            address = '127.0.0.1:1338'
        "#).unwrap();

        assert_eq!(friend_file.public_key, "qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqo");
        assert_eq!(friend_file.relays.len(), 2);

        assert_eq!(friend_file.relays[0].public_key, "qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqo");
        assert_eq!(friend_file.relays[0].address, "127.0.0.1:1337");

        assert_eq!(friend_file.relays[1].public_key, "u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7s");
        assert_eq!(friend_file.relays[1].address, "127.0.0.1:1338");

    }

    #[test]
    fn test_store_load_friend() {
        // Create a temporary directory:
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("friend_address_file");

        let relay_address0 = RelayAddress {
            public_key: PublicKey::from(&[0xaa; PUBLIC_KEY_LEN]),
            address: "127.0.0.1:1337".to_owned().try_into().unwrap(),
        };

        let relay_address1 = RelayAddress {
            public_key: PublicKey::from(&[0xbb; PUBLIC_KEY_LEN]),
            address: "127.0.0.1:1338".to_owned().try_into().unwrap(),
        };

        let friend_address = FriendAddress {
            public_key: PublicKey::from(&[0xaa; PUBLIC_KEY_LEN]),
            relays: vec![relay_address0, relay_address1],
        };

        store_friend_to_file(&friend_address, &file_path).unwrap();
        let friend_address2 = load_friend_from_file(&file_path).unwrap();

        assert_eq!(friend_address, friend_address2);
    }
}



