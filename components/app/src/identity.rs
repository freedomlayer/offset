use std::path::Path;
use futures::task::{Spawn, SpawnExt};

use identity::{create_identity, IdentityClient};

use bin::load_identity_from_file;

#[derive(Debug)]
pub enum IdentityFromFileError {
    LoadFileError,
    CreateIdentityError,
}

pub fn identity_from_file<S>(idfile_path: &Path, 
                            mut spawner: S) 
            -> Result<IdentityClient, IdentityFromFileError>  
where
    S: Spawn,
{

    let identity = load_identity_from_file(idfile_path)
        .map_err(|_| IdentityFromFileError::LoadFileError)?;

    // Spawn identity service:
    let (sender, identity_loop) = create_identity(identity);
    spawner.spawn(identity_loop)
        .map_err(|_| IdentityFromFileError::CreateIdentityError)?;
    Ok(IdentityClient::new(sender))
}

