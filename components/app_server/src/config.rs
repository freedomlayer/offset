// use std::collections::HashMap;

// use crypto::identity::PublicKey;

// TODO:  Should we move this struct to offst-proto?
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct AppPermissions {
    /// Receives reports about state
    pub reports: bool,
    /// Can request routes
    pub routes: bool,
    /// Can send credits
    pub send_funds: bool,
    /// Can configure friends
    pub config: bool,
}

/*
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

*/


// TODO: Write code that reads the AppServer configuration from a file.
