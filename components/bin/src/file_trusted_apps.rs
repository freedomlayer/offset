use std::collections::HashMap;

use futures::StreamExt;

use async_std::fs;
use async_std::path::{PathBuf, Path};

use derive_more::From;

use common::conn::BoxFuture;

use crate::node::TrustedApps;

use proto::file::TrustedAppFile;
use proto::crypto::PublicKey;
use proto::ser_string::{deserialize_from_string, StringSerdeError};
use proto::app_server::messages::AppPermissions;


#[derive(Debug, From)]
enum FileTrustedAppsError {
    AsyncStdIoError(async_std::io::Error),
    StringSerdeError(StringSerdeError),
}

/// Load all trusted applications files from a given directory.
async fn load_trusted_apps(dir_path: &Path) -> Result<HashMap<PublicKey, AppPermissions>, FileTrustedAppsError> {
    let mut res_trusted = HashMap::new();
    let mut dir = fs::read_dir(dir_path).await?;
    while let Some(entry) = dir.next().await {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir().await {
            continue;
        }

        let trusted_app_file: TrustedAppFile = deserialize_from_string(&fs::read_to_string(&path).await?)?;
        res_trusted.insert(trusted_app_file.public_key, trusted_app_file.permissions);
    }
    Ok(res_trusted)
}

/// Trusted apps checker that is stored as files in a directory.
/// Directory structure:
///
/// - root_dir
///     - trusted_app_file1
///     - trusted_app_file2
///     - trusted_app_file3
///     - ...
///
/// Where each trusted_app_file corresponds to the permissions of one app.
#[derive(Debug, Clone)]
pub struct FileTrustedApps {
    trusted_apps_path: PathBuf,
}

impl FileTrustedApps {
    pub fn new(trusted_apps_path: PathBuf) -> Self {
        Self { trusted_apps_path }
    }
}

impl TrustedApps for FileTrustedApps {
    /// Get the permissions of an app. Returns None if the app is not trusted at all.
    fn app_permissions<'a>(&'a mut self, app_public_key: &'a PublicKey) -> BoxFuture<'a, Option<AppPermissions>> {
        Box::pin(async move {
            let trusted_map = match load_trusted_apps(&self.trusted_apps_path).await {
                Ok(trusted_map) => trusted_map,
                Err(e) => {
                    error!("load_trusted_apps() failed: {:?}",e);
                    return None;
                }
            };
            let opt_app_permissions = trusted_map.get(app_public_key).cloned();
            opt_app_permissions
        })
    }
}
