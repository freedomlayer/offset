use std::fs;
use std::path::Path;

use futures::task::{Spawn, SpawnExt};

use derive_more::From;

use identity::{create_identity, IdentityClient};

use crypto::identity::SoftwareEd25519Identity;

use proto::file::IdentityFile;
use proto::ser_string::{deserialize_from_string, StringSerdeError};

#[derive(Debug, From)]
pub enum IdentityFromFileError {
    LoadFileError,
    LoadIdentityError,
    CreateIdentityError,
    StringSerdeError(StringSerdeError),
    IoError(std::io::Error),
}

pub fn identity_from_file<S>(
    idfile_path: &Path,
    spawner: S,
) -> Result<IdentityClient, IdentityFromFileError>
where
    S: Spawn,
{
    // Parse identity file:
    let identity_file: IdentityFile = deserialize_from_string(&fs::read_to_string(&idfile_path)?)?;
    let identity = SoftwareEd25519Identity::from_private_key(&identity_file.private_key)
        .map_err(|_| IdentityFromFileError::LoadIdentityError)?;

    // Spawn identity service:
    let (sender, identity_loop) = create_identity(identity);
    spawner
        .spawn(identity_loop)
        .map_err(|_| IdentityFromFileError::CreateIdentityError)?;
    Ok(IdentityClient::new(sender))
}
