use std::io::{self, Write};
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;

use crypto::identity::{SoftwareEd25519Identity, Identity};

/// Load an identity from a file
/// The file stores the private key according to PKCS#8.
/// TODO: Be able to read base64 style PKCS#8 files.
pub fn load_identity_from_file(path_buf: PathBuf) -> Option<impl Identity> {
    let mut file = File::open(path_buf).ok()?;
    let mut buf = [0u8; 85]; // TODO: Make this more generic?
    file.read(&mut buf).ok()?;
    SoftwareEd25519Identity::from_pkcs8(&buf).ok()
}


pub fn store_identity_to_file(pkcs8_buf: [u8; 85], path_buf: PathBuf) -> Result<(),io::Error> {
    let mut file = File::create(path_buf)?;
    file.write(&pkcs8_buf)?;
    Ok(())
}
