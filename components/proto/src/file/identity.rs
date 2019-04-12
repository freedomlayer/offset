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

// TODO: Tests
