use std::fs::{self, File};
use std::io::{self, Write};
use std::path::Path;

use toml;

use crypto::identity::{Identity, PrivateKey, SoftwareEd25519Identity};

use crate::file::ser_string::{from_base64, to_base64, SerStringError};
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
    Pkcs8ParseError,
}

/// A helper structure for serialize and deserializing IdentityAddress.
#[derive(Serialize, Deserialize)]
pub struct IdentityFile {
    #[serde(serialize_with = "to_base64", deserialize_with = "from_base64")]
    pub private_key: PrivateKey,
}

impl From<SerStringError> for IdentityFileError {
    fn from(_e: SerStringError) -> Self {
        IdentityFileError::SerStringError
    }
}

// TODO: Rename this function:
/// Load Identity from a file
pub fn load_raw_identity_from_file(path: &Path) -> Result<PrivateKey, IdentityFileError> {
    let data = fs::read_to_string(&path)?;
    let identity_file: IdentityFile = toml::from_str(&data)?;

    Ok(identity_file.private_key)
}

/// Store Identity to file
pub fn store_raw_identity_to_file(
    private_key: &PrivateKey,
    path: &Path,
) -> Result<(), IdentityFileError> {
    let identity_file = IdentityFile {
        private_key: private_key.clone(),
    };

    let data = toml::to_string(&identity_file)?;

    let mut file = File::create(path)?;
    file.write_all(&data.as_bytes())?;

    Ok(())
}

/// Load an identity from a file
/// The file stores the private key according to PKCS#8.
pub fn load_identity_from_file(path: &Path) -> Result<impl Identity, IdentityFileError> {
    let private_key = load_raw_identity_from_file(path)?;
    SoftwareEd25519Identity::from_private_key(&private_key)
        .map_err(|_| IdentityFileError::Pkcs8ParseError)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    /*
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
    */

    #[test]
    fn test_store_load_identity() {
        // Create a temporary directory:
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("identity_file");

        let private_key = PrivateKey::from([33u8; 85]);

        store_raw_identity_to_file(&private_key, &file_path).unwrap();
        let private_key2 = load_raw_identity_from_file(&file_path).unwrap();

        // We convert to vec here because [u8; 85] doesn't implement PartialEq
        assert_eq!(private_key.to_vec(), private_key2.to_vec());
    }
}
