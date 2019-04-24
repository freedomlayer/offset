/*
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::Path;

use crypto::identity::{Identity, SoftwareEd25519Identity};

#[derive(Debug, From)]
pub enum IdentityFileError {
    IoError(io::Error),
    ParseError,
}

/// Load an identity from a file
/// The file stores the private key according to PKCS#8.
/// TODO: Be able to read base64 style PKCS#8 files.
pub fn load_identity_from_file(path: &Path) -> Result<impl Identity, IdentityFileError> {
    let file = fs::read(path)?;
    SoftwareEd25519Identity::from_pkcs8(&file).map_err(|_| IdentityFileError::ParseError)
}

pub fn store_identity_to_file(pkcs8_buf: [u8; 85], path: &Path) -> Result<(), IdentityFileError> {
    let mut file = File::create(path)?;
    file.write_all(&pkcs8_buf)?;
    Ok(())
}
*/

// ------------------------------------------
// ------------------------------------------

// TODO: Tests

use std::fs::{self, File};
use std::io::{self, Write};
use std::path::Path;

use crate::file::ser_string::{private_key_to_string, string_to_private_key, SerStringError};
use toml;

use crate::net::messages::NetAddressError;

#[derive(Debug, From)]
pub enum IdentityFileError {
    IoError(io::Error),
    TomlDeError(toml::de::Error),
    TomlSeError(toml::ser::Error),
    SerStringError,
    ParseSocketAddrError,
    InvalidPublicKey,
    NetAddressError(NetAddressError),
}

/// A helper structure for serialize and deserializing IdentityAddress.
#[derive(Serialize, Deserialize)]
pub struct IdentityFile {
    pub private_key: String,
}

impl From<SerStringError> for IdentityFileError {
    fn from(_e: SerStringError) -> Self {
        IdentityFileError::SerStringError
    }
}

/// Load Identity from a file
pub fn load_identity_from_file(path: &Path) -> Result<[u8; 85], IdentityFileError> {
    let data = dbg!(fs::read_to_string(&path)?);
    let identity_file: IdentityFile = toml::from_str(&data)?;

    // Decode public key:
    let private_key = string_to_private_key(&identity_file.private_key)?;
    Ok(private_key)
}

/// Store Identity to file
pub fn store_identity_to_file(identity: &[u8; 85], path: &Path) -> Result<(), IdentityFileError> {
    let identity_file = IdentityFile {
        private_key: private_key_to_string(&identity),
    };

    let data = toml::to_string(&identity_file)?;

    let mut file = File::create(path)?;
    file.write_all(&data.as_bytes())?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_identity_file_basic() {
        let identity_file: IdentityFile = toml::from_str(
            r#"
            private_key = 'private_key_string'
            "#,
        )
        .unwrap();

        assert_eq!(identity_file.private_key, "private_key_string");
    }

    #[test]
    fn test_store_load_identity() {
        // Create a temporary directory:
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("identity_file");

        let identity = [33u8; 85];

        store_identity_to_file(&identity, &file_path).unwrap();
        let identity2 = load_identity_from_file(&file_path).unwrap();

        // We convert to vec here because [u8; 85] doesn't implement PartialEq
        assert_eq!(identity.to_vec(), identity2.to_vec());
    }
}
