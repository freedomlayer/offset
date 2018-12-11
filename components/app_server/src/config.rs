use std::collections::HashMap;

use crypto::identity::PublicKey;

pub enum AppPermissions {
    /// Receives updates about state, and can receive delegation:
    Read,
    /// Allowed to do anything:
    Full,
}

pub struct AppConfig {
    /// Application name
    pub name: String,
    pub permissions: AppPermissions,
}

/// Configuration for AppServer
pub struct AppServerConfig {
    pub apps: HashMap<PublicKey, AppConfig>,
}


// TODO: Write code that reads the AppServer configuration from a file.
