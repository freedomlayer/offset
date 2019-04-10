use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use crate::file::ser_string::{public_key_to_string, string_to_public_key, SerStringError};
use toml;

use crate::app_server::messages::AppPermissions;
use crypto::identity::PublicKey;

#[derive(Debug, From)]
pub enum AppFileError {
    IoError(io::Error),
    TomlDeError(toml::de::Error),
    TomlSeError(toml::ser::Error),
    SerStringError,
    InvalidPublicKey,
}

/// A helper structure for serialize and deserializing IndexServerAddress.
#[derive(Debug, Serialize, Deserialize)]
pub struct TrustedAppFile {
    public_key: String,
    permissions: AppPermissions,
}

#[derive(Debug, PartialEq, Eq)]
pub struct TrustedApp {
    pub public_key: PublicKey,
    pub permissions: AppPermissions,
}

impl From<SerStringError> for AppFileError {
    fn from(_e: SerStringError) -> Self {
        AppFileError::SerStringError
    }
}

/// Load a TrustedApp from a file
pub fn load_trusted_app_from_file(path: &Path) -> Result<TrustedApp, AppFileError> {
    let data = fs::read_to_string(&path)?;
    let trusted_app_file: TrustedAppFile = toml::from_str(&data)?;

    let public_key = string_to_public_key(&trusted_app_file.public_key)?;

    Ok(TrustedApp {
        public_key,
        permissions: trusted_app_file.permissions,
    })
}

/// Store TrustedApp to a file
pub fn store_trusted_app_to_file(
    trusted_app: &TrustedApp,
    path: &Path,
) -> Result<(), AppFileError> {
    let TrustedApp {
        ref public_key,
        ref permissions,
    } = trusted_app;

    let trusted_app_file = TrustedAppFile {
        public_key: public_key_to_string(&public_key),
        permissions: permissions.clone(),
    };

    let data = toml::to_string(&trusted_app_file)?;

    let mut file = File::create(path)?;
    file.write_all(&data.as_bytes())?;

    Ok(())
}

#[derive(Debug)]
pub enum AppDirectoryError {
    IoError(io::Error),
    InvalidDirectory(io::Error),
    InvalidFile(PathBuf, AppFileError),
}

impl From<io::Error> for AppDirectoryError {
    fn from(e: io::Error) -> Self {
        AppDirectoryError::IoError(e)
    }
}

/// Load all trusted applications files from a given directory.
pub fn load_trusted_apps(dir_path: &Path) -> Result<Vec<TrustedApp>, AppDirectoryError> {
    let mut res_trusted = Vec::new();
    for entry in fs::read_dir(dir_path).map_err(AppDirectoryError::InvalidDirectory)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            continue;
        }
        res_trusted.push(
            load_trusted_app_from_file(&path)
                .map_err(|e| AppDirectoryError::InvalidFile(path, e))?,
        );
    }
    Ok(res_trusted)
}

#[cfg(test)]
mod tests {
    use super::*;

    use crypto::identity::PUBLIC_KEY_LEN;
    use tempfile::tempdir;

    #[test]
    fn test_store_load_trusted_app() {
        // Create a temporary directory:
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("trusted_app_file");

        let permissions = AppPermissions {
            routes: true,
            send_funds: false,
            config: true,
        };
        let trusted_app = TrustedApp {
            public_key: PublicKey::from(&[0xaa; PUBLIC_KEY_LEN]),
            permissions,
        };

        store_trusted_app_to_file(&trusted_app, &file_path).unwrap();
        let trusted_app2 = load_trusted_app_from_file(&file_path).unwrap();

        assert_eq!(trusted_app, trusted_app2);
    }

    #[test]
    fn test_load_trusted_apps() {
        // Create a temporary directory:
        let dir = tempdir().unwrap();

        let file_path = dir.path().join("trusted_app1");
        let permissions = AppPermissions {
            routes: true,
            send_funds: false,
            config: true,
        };
        let trusted_app1 = TrustedApp {
            public_key: PublicKey::from(&[0xaa; PUBLIC_KEY_LEN]),
            permissions,
        };
        store_trusted_app_to_file(&trusted_app1, &file_path).unwrap();

        let file_path = dir.path().join("trusted_app2");
        let permissions = AppPermissions {
            routes: false,
            send_funds: true,
            config: false,
        };
        let trusted_app2 = TrustedApp {
            public_key: PublicKey::from(&[0xbb; PUBLIC_KEY_LEN]),
            permissions,
        };
        store_trusted_app_to_file(&trusted_app2, &file_path).unwrap();

        let trusted = load_trusted_apps(&dir.path()).unwrap();
        assert_eq!(trusted.len(), 2);
        let mut pks = vec![trusted[0].public_key[0], trusted[1].public_key[0]];
        pks.sort();
        assert_eq!(pks, vec![0xaa, 0xbb]);
    }
}
