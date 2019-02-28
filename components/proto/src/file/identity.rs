use std::io::{self, Write};
use std::fs::File;
use std::io::Read;
use std::path::Path;

use crypto::identity::{SoftwareEd25519Identity, Identity};

#[derive(Debug)]
pub enum IdentityFileError {
    IoError(io::Error),
    ParseError,
}

impl From<io::Error> for IdentityFileError {
    fn from(io_error: io::Error) -> Self {
        IdentityFileError::IoError(io_error)
    }
}

/// Load an identity from a file
/// The file stores the private key according to PKCS#8.
/// TODO: Be able to read base64 style PKCS#8 files.
pub fn load_identity_from_file(path: &Path) 
    -> Result<impl Identity, IdentityFileError> {

    let mut file = File::open(path)?;
    let mut buf = [0u8; 85]; // TODO: Make this more generic?
    file.read(&mut buf)?;
    SoftwareEd25519Identity::from_pkcs8(&buf)
        .map_err(|_| IdentityFileError::ParseError)
}


pub fn store_identity_to_file(pkcs8_buf: [u8; 85], path: &Path) 
    -> Result<(), IdentityFileError> {

    let mut file = File::create(path)?;
    file.write(&pkcs8_buf)?;
    Ok(())
}


// TODO: Tests
