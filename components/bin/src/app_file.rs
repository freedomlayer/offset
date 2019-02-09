use std::io::{self, Write};
use std::fs::{self, File};
use std::path::Path;

use toml;
use base64::{self, URL_SAFE_NO_PAD};

use crypto::identity::{PublicKey, PUBLIC_KEY_LEN};
use proto::app_server::messages::AppPermissions;

#[derive(Debug)]
pub enum AppFileError {
    IoError(io::Error),
    TomlDeError(toml::de::Error),
    TomlSeError(toml::ser::Error),
    Base64DecodeError(base64::DecodeError),
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

impl From<io::Error> for AppFileError {
    fn from(e: io::Error) -> Self {
        AppFileError::IoError(e)
    }
}

impl From<toml::de::Error> for AppFileError {
    fn from(e: toml::de::Error) -> Self {
        AppFileError::TomlDeError(e)
    }
}

impl From<toml::ser::Error> for AppFileError {
    fn from(e: toml::ser::Error) -> Self {
        AppFileError::TomlSeError(e)
    }
}

impl From<base64::DecodeError> for AppFileError {
    fn from(e: base64::DecodeError) -> Self {
        AppFileError::Base64DecodeError(e)
    }
}


/// Load a TrustedApp from a file
pub fn load_trusted_app_from_file(path: &Path) -> Result<TrustedApp, AppFileError> {
    let data = fs::read_to_string(&path)?;
    let trusted_app_file: TrustedAppFile = toml::from_str(&data)?;

    // Decode public key:
    let public_key_vec = base64::decode_config(&trusted_app_file.public_key, URL_SAFE_NO_PAD)?;
    // TODO: A more idiomatic way to do this?
    if public_key_vec.len() != PUBLIC_KEY_LEN {
        return Err(AppFileError::InvalidPublicKey);
    }
    let mut public_key_array = [0u8; PUBLIC_KEY_LEN];
    public_key_array.copy_from_slice(&public_key_vec[0 .. PUBLIC_KEY_LEN]);
    let public_key = PublicKey::from(&public_key_array);

    Ok(TrustedApp {
        public_key,
        permissions: trusted_app_file.permissions,
    })
}

/// Store TrustedApp to a file
pub fn store_trusted_app_to_file(trusted_app: &TrustedApp, path: &Path)
    -> Result<(), AppFileError> {

    let TrustedApp {ref public_key, ref permissions} = trusted_app;

    let trusted_app_file = TrustedAppFile {
        public_key: base64::encode_config(&public_key, URL_SAFE_NO_PAD),
        permissions: permissions.clone(),
    };

    let data = toml::to_string(&trusted_app_file)?;

    let mut file = File::create(path)?;
    file.write(&data.as_bytes())?;

    Ok(())
}

/// Load all trusted applications files from a given directory.
pub fn load_trusted_apps(dir_path: &Path) -> Result<Vec<TrustedApp>, AppFileError> {
    let mut res_trusted = Vec::new();
    for entry in fs::read_dir(dir_path)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            continue;
        }
        res_trusted.push(load_trusted_app_from_file(&path)?);
    }
    Ok(res_trusted)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_store_load_trusted_app() {
        // Create a temporary directory:
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("trusted_app_file");

        let permissions = AppPermissions {
            reports: false,
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
            reports: false,
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
            reports: true,
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
