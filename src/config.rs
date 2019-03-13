use clap::ArgMatches;

use app::{NodeConnection, AppConfig};

#[derive(Debug)]
pub enum ConfigError {
    /// No permissions to configure node
    NoPermissions,
}

async fn config_add_relay<'a>(_matches: &'a ArgMatches<'a>, _app_config: AppConfig) -> Result<(), ConfigError> {
    unimplemented!();
}

async fn config_remove_relay<'a>(_matches: &'a ArgMatches<'a>, _app_config: AppConfig) -> Result<(), ConfigError> {
    unimplemented!();
}

async fn config_add_index<'a>(_matches: &'a ArgMatches<'a>, _app_config: AppConfig) -> Result<(), ConfigError> {
    unimplemented!();
}

async fn config_remove_index<'a>(_matches: &'a ArgMatches<'a>, _app_config: AppConfig) -> Result<(), ConfigError> {
    unimplemented!();
}

async fn config_add_friend<'a>(_matches: &'a ArgMatches<'a>, _app_config: AppConfig) -> Result<(), ConfigError> {
    unimplemented!();
}

async fn config_update_friend<'a>(_matches: &'a ArgMatches<'a>, _app_config: AppConfig) -> Result<(), ConfigError> {
    unimplemented!();
}

async fn config_remove_friend<'a>(_matches: &'a ArgMatches<'a>, _app_config: AppConfig) -> Result<(), ConfigError> {
    unimplemented!();
}

async fn config_enable_friend<'a>(_matches: &'a ArgMatches<'a>, _app_config: AppConfig) -> Result<(), ConfigError> {
    unimplemented!();
}

async fn config_disable_friend<'a>(_matches: &'a ArgMatches<'a>, _app_config: AppConfig) -> Result<(), ConfigError> {
    unimplemented!();
}

async fn config_open_friend<'a>(_matches: &'a ArgMatches<'a>, _app_config: AppConfig) -> Result<(), ConfigError> {
    unimplemented!();
}

async fn config_close_friend<'a>(_matches: &'a ArgMatches<'a>, _app_config: AppConfig) -> Result<(), ConfigError> {
    unimplemented!();
}

async fn config_set_friend_max_debt<'a>(_matches: &'a ArgMatches<'a>, _app_config: AppConfig) -> Result<(), ConfigError> {
    unimplemented!();
}

async fn config_reset_friend<'a>(_matches: &'a ArgMatches<'a>, _app_config: AppConfig) -> Result<(), ConfigError> {
    unimplemented!();
}

pub async fn config<'a>(matches: &'a ArgMatches<'a>, mut node_connection: NodeConnection) -> Result<(), ConfigError> {
    let app_config = match node_connection.config() {
        Some(app_config) => app_config.clone(),
        None => return Err(ConfigError::NoPermissions),
    };

    match matches.subcommand() {
        ("add-relay", Some(matches)) => await!(config_add_relay(matches, app_config))?,
        ("remove-relay", Some(matches)) => await!(config_remove_relay(matches, app_config))?,
        ("add-index", Some(matches)) => await!(config_add_index(matches, app_config))?,
        ("remove-index", Some(matches)) => await!(config_remove_index(matches, app_config))?,
        ("add-friend", Some(matches)) => await!(config_add_friend(matches, app_config))?,
        ("update-friend", Some(matches)) => await!(config_update_friend(matches, app_config))?,
        ("remove-friend", Some(matches)) => await!(config_remove_friend(matches, app_config))?,
        ("enable-friend", Some(matches)) => await!(config_enable_friend(matches, app_config))?,
        ("disable-friend", Some(matches)) => await!(config_disable_friend(matches, app_config))?,
        ("open-friend", Some(matches)) => await!(config_open_friend(matches, app_config))?,
        ("close-friend", Some(matches)) => await!(config_close_friend(matches, app_config))?,
        ("set-friend-max-debt", Some(matches)) => await!(config_set_friend_max_debt(matches, app_config))?,
        ("reset-friend", Some(matches)) => await!(config_reset_friend(matches, app_config))?,
        _ => unreachable!(),
    }

    Ok(())
}
