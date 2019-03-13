use std::path::PathBuf;

use clap::ArgMatches;

use app::{NodeConnection, AppConfig, 
    NamedRelayAddress, NamedIndexServerAddress, 
    load_relay_from_file, load_index_server_from_file};
use app::report::NodeReport;

#[derive(Debug)]
pub enum ConfigError {
    /// No permissions to configure node
    NoPermissions,
    GetReportError,
    RelayNameAlreadyExists,
    RelayFileNotFound,
    LoadRelayFromFileError,
    AppConfigError,
    RelayNameNotFound,
    IndexNameAlreadyExists,
    IndexFileNotFound,
    LoadIndexFromFileError,
}

async fn config_add_relay<'a>(matches: &'a ArgMatches<'a>, 
                              mut app_config: AppConfig,
                              node_report: NodeReport) -> Result<(), ConfigError> {

    let relay_file = matches.value_of("relay_file").unwrap();
    let relay_name = matches.value_of("relay_name").unwrap();

    for named_relay_address in node_report.funder_report.relays {
        if named_relay_address.name == relay_name {
            return Err(ConfigError::RelayNameAlreadyExists);
        }
    }

    let relay_pathbuf = PathBuf::from(relay_file);
    if !relay_pathbuf.exists() {
        return Err(ConfigError::RelayFileNotFound);
    }

    let relay_address = load_relay_from_file(&relay_pathbuf)
        .map_err(|_| ConfigError::LoadRelayFromFileError)?;

    let named_relay_address = NamedRelayAddress {
        public_key: relay_address.public_key,
        address: relay_address.address,
        name: relay_name.to_owned(),
    };

    await!(app_config.add_relay(named_relay_address))
        .map_err(|_| ConfigError::AppConfigError)
}

async fn config_remove_relay<'a>(matches: &'a ArgMatches<'a>, 
                                 mut app_config: AppConfig,
                                 node_report: NodeReport) -> Result<(), ConfigError> {

    let relay_name = matches.value_of("relay_name").unwrap();

    let mut opt_relay_public_key = None;
    for named_relay_address in node_report.funder_report.relays {
        if named_relay_address.name == relay_name {
            opt_relay_public_key = Some(named_relay_address.public_key.clone());
        }
    }

    let relay_public_key = opt_relay_public_key
        .ok_or(ConfigError::RelayNameNotFound)?;

    await!(app_config.remove_relay(relay_public_key))
        .map_err(|_| ConfigError::AppConfigError)
}

async fn config_add_index<'a>(matches: &'a ArgMatches<'a>, 
                              mut app_config: AppConfig,
                              node_report: NodeReport) -> Result<(), ConfigError> {

    let index_file = matches.value_of("index_file").unwrap();
    let index_name = matches.value_of("index_name").unwrap();

    for named_index_server_address in node_report.index_client_report.index_servers {
        if named_index_server_address.name == index_name {
            return Err(ConfigError::IndexNameAlreadyExists);
        }
    }

    let index_pathbuf = PathBuf::from(index_file);
    if !index_pathbuf.exists() {
        return Err(ConfigError::IndexFileNotFound);
    }

    let index_server_address = load_index_server_from_file(&index_pathbuf)
        .map_err(|_| ConfigError::LoadIndexFromFileError)?;

    let named_index_server_address = NamedIndexServerAddress {
        public_key: index_server_address.public_key,
        address: index_server_address.address,
        name: index_name.to_owned(),
    };

    await!(app_config.add_index_server(named_index_server_address))
        .map_err(|_| ConfigError::AppConfigError)
}

async fn config_remove_index<'a>(matches: &'a ArgMatches<'a>, 
                                 mut app_config: AppConfig,
                                 node_report: NodeReport) -> Result<(), ConfigError> {

    let index_name = matches.value_of("index_name").unwrap();

    let mut opt_index_public_key = None;
    for named_index_server_address in node_report.index_client_report.index_servers {
        if named_index_server_address.name == index_name {
            opt_index_public_key = Some(named_index_server_address.public_key.clone());
        }
    }

    let index_public_key = opt_index_public_key
        .ok_or(ConfigError::RelayNameNotFound)?;

    await!(app_config.remove_index_server(index_public_key))
        .map_err(|_| ConfigError::AppConfigError)
}

async fn config_add_friend<'a>(_matches: &'a ArgMatches<'a>, 
                               _app_config: AppConfig,
                               _node_report: NodeReport) -> Result<(), ConfigError> {
    unimplemented!();
}

async fn config_update_friend<'a>(_matches: &'a ArgMatches<'a>, 
                                  _app_config: AppConfig,
                                  _node_report: NodeReport) -> Result<(), ConfigError> {
    unimplemented!();
}

async fn config_remove_friend<'a>(_matches: &'a ArgMatches<'a>, 
                                  _app_config: AppConfig,
                                  _node_report: NodeReport) -> Result<(), ConfigError> {
    unimplemented!();
}

async fn config_enable_friend<'a>(_matches: &'a ArgMatches<'a>, 
                                  _app_config: AppConfig,
                                  _node_report: NodeReport) -> Result<(), ConfigError> {
    unimplemented!();
}

async fn config_disable_friend<'a>(_matches: &'a ArgMatches<'a>, 
                                   _app_config: AppConfig,
                                   _node_report: NodeReport) -> Result<(), ConfigError> {
    unimplemented!();
}

async fn config_open_friend<'a>(_matches: &'a ArgMatches<'a>, 
                                _app_config: AppConfig,
                                _node_report: NodeReport) -> Result<(), ConfigError> {
    unimplemented!();
}

async fn config_close_friend<'a>(_matches: &'a ArgMatches<'a>, 
                                 _app_config: AppConfig,
                                 _node_report: NodeReport) -> Result<(), ConfigError> {
    unimplemented!();
}

async fn config_set_friend_max_debt<'a>(_matches: &'a ArgMatches<'a>, 
                                        _app_config: AppConfig,
                                        _node_report: NodeReport) -> Result<(), ConfigError> {
    unimplemented!();
}

async fn config_reset_friend<'a>(_matches: &'a ArgMatches<'a>, 
                                 _app_config: AppConfig,
                                 _node_report: NodeReport) -> Result<(), ConfigError> {
    unimplemented!();
}

pub async fn config<'a>(matches: &'a ArgMatches<'a>, mut node_connection: NodeConnection) -> Result<(), ConfigError> {
    let app_config = node_connection.config()
        .ok_or(ConfigError::NoPermissions)?
        .clone();

    // Obtain current report:
    let app_report = node_connection.report();
    let (node_report, incoming_mutations) = await!(app_report.incoming_reports())
        .map_err(|_| ConfigError::GetReportError)?;
    // We currently don't need live updates about report mutations:
    drop(incoming_mutations);
    drop(app_report);

    match matches.subcommand() {
        ("add-relay", Some(matches)) => await!(config_add_relay(matches, app_config, node_report))?,
        ("remove-relay", Some(matches)) => await!(config_remove_relay(matches, app_config, node_report))?,
        ("add-index", Some(matches)) => await!(config_add_index(matches, app_config, node_report))?,
        ("remove-index", Some(matches)) => await!(config_remove_index(matches, app_config, node_report))?,
        ("add-friend", Some(matches)) => await!(config_add_friend(matches, app_config, node_report))?,
        ("update-friend", Some(matches)) => await!(config_update_friend(matches, app_config, node_report))?,
        ("remove-friend", Some(matches)) => await!(config_remove_friend(matches, app_config, node_report))?,
        ("enable-friend", Some(matches)) => await!(config_enable_friend(matches, app_config, node_report))?,
        ("disable-friend", Some(matches)) => await!(config_disable_friend(matches, app_config, node_report))?,
        ("open-friend", Some(matches)) => await!(config_open_friend(matches, app_config, node_report))?,
        ("close-friend", Some(matches)) => await!(config_close_friend(matches, app_config, node_report))?,
        ("set-friend-max-debt", Some(matches)) => await!(config_set_friend_max_debt(matches, app_config, node_report))?,
        ("reset-friend", Some(matches)) => await!(config_reset_friend(matches, app_config, node_report))?,
        _ => unreachable!(),
    }

    Ok(())
}
