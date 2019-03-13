use clap::ArgMatches;

use app::{NodeConnection, AppConfig};
use app::report::NodeReport;

#[derive(Debug)]
pub enum ConfigError {
    /// No permissions to configure node
    NoPermissions,
    GetReportError,
}

async fn config_add_relay<'a>(_matches: &'a ArgMatches<'a>, 
                              _app_config: AppConfig,
                              _node_report: NodeReport) -> Result<(), ConfigError> {
    unimplemented!();
}

async fn config_remove_relay<'a>(_matches: &'a ArgMatches<'a>, 
                                 _app_config: AppConfig,
                                 _node_report: NodeReport) -> Result<(), ConfigError> {
    unimplemented!();
}

async fn config_add_index<'a>(_matches: &'a ArgMatches<'a>, 
                              _app_config: AppConfig,
                              _node_report: NodeReport) -> Result<(), ConfigError> {
    unimplemented!();
}

async fn config_remove_index<'a>(_matches: &'a ArgMatches<'a>, 
                                 _app_config: AppConfig,
                                 _node_report: NodeReport) -> Result<(), ConfigError> {
    unimplemented!();
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
