use std::collections::HashMap;

use crypto::identity::PublicKey;

#[derive(Debug)]
pub struct AppPermissions {
    /// Receives reports about state
    reports: bool,
    /// Can request routes
    routes: bool,
    /// Can send credits
    send_credits: bool,
    /// Can configure friends
    config: bool,
}

#[derive(Debug)]
pub struct AppConfig {
    /// Application name
    pub name: String,
    pub permissions: AppPermissions,
}

/// Configuration for AppServer
#[derive(Debug)]
pub struct AppServerConfig {
    pub apps: HashMap<PublicKey, AppConfig>,
}


// TODO: Write code that reads the AppServer configuration from a file.
