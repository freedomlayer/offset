use std::path::PathBuf;

use clap::ArgMatches;

use app::report::{ChannelStatusReport, NodeReport};
use app::{
    load_friend_from_file, load_index_server_from_file, load_relay_from_file, AppConfig,
    NamedIndexServerAddress, NamedRelayAddress, NodeConnection, PublicKey,
};

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
    FriendNameAlreadyExists,
    ParseBalanceError,
    FriendFileNotFound,
    LoadFriendFromFileError,
    FriendPublicKeyMismatch,
    FriendNameNotFound,
    ParseMaxDebtError,
    ChannelNotInconsistent,
    UnknownRemoteResetTerms,
}

async fn config_add_relay<'a>(
    matches: &'a ArgMatches<'a>,
    mut app_config: AppConfig,
    node_report: NodeReport,
) -> Result<(), ConfigError> {
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

    let relay_address =
        load_relay_from_file(&relay_pathbuf).map_err(|_| ConfigError::LoadRelayFromFileError)?;

    let named_relay_address = NamedRelayAddress {
        public_key: relay_address.public_key,
        address: relay_address.address,
        name: relay_name.to_owned(),
    };

    await!(app_config.add_relay(named_relay_address)).map_err(|_| ConfigError::AppConfigError)
}

async fn config_remove_relay<'a>(
    matches: &'a ArgMatches<'a>,
    mut app_config: AppConfig,
    node_report: NodeReport,
) -> Result<(), ConfigError> {
    let relay_name = matches.value_of("relay_name").unwrap();

    let mut opt_relay_public_key = None;
    for named_relay_address in node_report.funder_report.relays {
        if named_relay_address.name == relay_name {
            opt_relay_public_key = Some(named_relay_address.public_key.clone());
        }
    }

    let relay_public_key = opt_relay_public_key.ok_or(ConfigError::RelayNameNotFound)?;

    await!(app_config.remove_relay(relay_public_key)).map_err(|_| ConfigError::AppConfigError)
}

async fn config_add_index<'a>(
    matches: &'a ArgMatches<'a>,
    mut app_config: AppConfig,
    node_report: NodeReport,
) -> Result<(), ConfigError> {
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

async fn config_remove_index<'a>(
    matches: &'a ArgMatches<'a>,
    mut app_config: AppConfig,
    node_report: NodeReport,
) -> Result<(), ConfigError> {
    let index_name = matches.value_of("index_name").unwrap();

    let mut opt_index_public_key = None;
    for named_index_server_address in node_report.index_client_report.index_servers {
        if named_index_server_address.name == index_name {
            opt_index_public_key = Some(named_index_server_address.public_key.clone());
        }
    }

    let index_public_key = opt_index_public_key.ok_or(ConfigError::RelayNameNotFound)?;

    await!(app_config.remove_index_server(index_public_key))
        .map_err(|_| ConfigError::AppConfigError)
}

async fn config_add_friend<'a>(
    matches: &'a ArgMatches<'a>,
    mut app_config: AppConfig,
    node_report: NodeReport,
) -> Result<(), ConfigError> {
    let friend_file = matches.value_of("friend_file").unwrap();
    let friend_name = matches.value_of("friend_name").unwrap();
    let friend_balance_str = matches.value_of("friend_balance").unwrap();

    let friend_balance = friend_balance_str
        .parse::<i128>()
        .map_err(|_| ConfigError::ParseBalanceError)?;

    for (_friend_public_key, friend_report) in node_report.funder_report.friends {
        if friend_report.name == friend_name {
            return Err(ConfigError::FriendNameAlreadyExists);
        }
    }

    let friend_pathbuf = PathBuf::from(friend_file);
    if !friend_pathbuf.exists() {
        return Err(ConfigError::FriendFileNotFound);
    }

    let friend_address =
        load_friend_from_file(&friend_pathbuf).map_err(|_| ConfigError::LoadFriendFromFileError)?;

    await!(app_config.add_friend(
        friend_address.public_key,
        friend_address.relays,
        friend_name.to_owned(),
        friend_balance
    ))
    .map_err(|_| ConfigError::AppConfigError)?;
    Ok(())
}

/// Find a friend's public key given his name
fn friend_public_key_by_name<'a>(
    node_report: &'a NodeReport,
    friend_name: &str,
) -> Option<&'a PublicKey> {
    // Search for the friend:
    for (friend_public_key, friend_report) in &node_report.funder_report.friends {
        if friend_report.name == friend_name {
            return Some(friend_public_key);
        }
    }
    None
}

async fn config_set_friend_relays<'a>(
    matches: &'a ArgMatches<'a>,
    mut app_config: AppConfig,
    node_report: NodeReport,
) -> Result<(), ConfigError> {
    let friend_file = matches.value_of("friend_file").unwrap();
    let friend_name = matches.value_of("friend_name").unwrap();

    let friend_public_key = friend_public_key_by_name(&node_report, friend_name)
        .ok_or(ConfigError::FriendNameNotFound)?
        .clone();

    let friend_pathbuf = PathBuf::from(friend_file);
    if !friend_pathbuf.exists() {
        return Err(ConfigError::FriendFileNotFound);
    }

    let friend_address =
        load_friend_from_file(&friend_pathbuf).map_err(|_| ConfigError::LoadFriendFromFileError)?;

    // Just in case, make sure that the the friend we know with this name
    // has the same public key as inside the provided file.
    if friend_address.public_key != friend_public_key {
        return Err(ConfigError::FriendPublicKeyMismatch);
    }

    await!(app_config.set_friend_relays(friend_public_key, friend_address.relays))
        .map_err(|_| ConfigError::AppConfigError)?;

    Ok(())
}

async fn config_remove_friend<'a>(
    matches: &'a ArgMatches<'a>,
    mut app_config: AppConfig,
    node_report: NodeReport,
) -> Result<(), ConfigError> {
    let friend_name = matches.value_of("friend_name").unwrap();

    let friend_public_key = friend_public_key_by_name(&node_report, friend_name)
        .ok_or(ConfigError::FriendNameNotFound)?
        .clone();

    await!(app_config.remove_friend(friend_public_key)).map_err(|_| ConfigError::AppConfigError)
}

async fn config_enable_friend<'a>(
    matches: &'a ArgMatches<'a>,
    mut app_config: AppConfig,
    node_report: NodeReport,
) -> Result<(), ConfigError> {
    let friend_name = matches.value_of("friend_name").unwrap();

    let friend_public_key = friend_public_key_by_name(&node_report, friend_name)
        .ok_or(ConfigError::FriendNameNotFound)?
        .clone();

    await!(app_config.enable_friend(friend_public_key)).map_err(|_| ConfigError::AppConfigError)
}

async fn config_disable_friend<'a>(
    matches: &'a ArgMatches<'a>,
    mut app_config: AppConfig,
    node_report: NodeReport,
) -> Result<(), ConfigError> {
    let friend_name = matches.value_of("friend_name").unwrap();

    let friend_public_key = friend_public_key_by_name(&node_report, friend_name)
        .ok_or(ConfigError::FriendNameNotFound)?
        .clone();

    await!(app_config.disable_friend(friend_public_key)).map_err(|_| ConfigError::AppConfigError)
}

async fn config_open_friend<'a>(
    matches: &'a ArgMatches<'a>,
    mut app_config: AppConfig,
    node_report: NodeReport,
) -> Result<(), ConfigError> {
    let friend_name = matches.value_of("friend_name").unwrap();

    let friend_public_key = friend_public_key_by_name(&node_report, friend_name)
        .ok_or(ConfigError::FriendNameNotFound)?
        .clone();

    await!(app_config.open_friend(friend_public_key)).map_err(|_| ConfigError::AppConfigError)
}

async fn config_close_friend<'a>(
    matches: &'a ArgMatches<'a>,
    mut app_config: AppConfig,
    node_report: NodeReport,
) -> Result<(), ConfigError> {
    let friend_name = matches.value_of("friend_name").unwrap();

    let friend_public_key = friend_public_key_by_name(&node_report, friend_name)
        .ok_or(ConfigError::FriendNameNotFound)?
        .clone();

    await!(app_config.close_friend(friend_public_key)).map_err(|_| ConfigError::AppConfigError)
}

async fn config_set_friend_max_debt<'a>(
    matches: &'a ArgMatches<'a>,
    mut app_config: AppConfig,
    node_report: NodeReport,
) -> Result<(), ConfigError> {
    let friend_name = matches.value_of("friend_name").unwrap();
    let max_debt_str = matches.value_of("max_debt").unwrap();

    let max_debt = max_debt_str
        .parse::<u128>()
        .map_err(|_| ConfigError::ParseMaxDebtError)?;

    let friend_public_key = friend_public_key_by_name(&node_report, friend_name)
        .ok_or(ConfigError::FriendNameNotFound)?
        .clone();

    await!(app_config.set_friend_remote_max_debt(friend_public_key, max_debt))
        .map_err(|_| ConfigError::AppConfigError)
}

async fn config_reset_friend<'a>(
    matches: &'a ArgMatches<'a>,
    mut app_config: AppConfig,
    node_report: NodeReport,
) -> Result<(), ConfigError> {
    let friend_name = matches.value_of("friend_name").unwrap();

    let mut opt_friend_pk_report = None;
    for (friend_public_key, friend_report) in &node_report.funder_report.friends {
        if friend_report.name == friend_name {
            opt_friend_pk_report = Some((friend_public_key, friend_report));
        }
    }

    let (friend_public_key, friend_report) =
        opt_friend_pk_report.ok_or(ConfigError::FriendNameNotFound)?;

    // Obtain the reset token
    // (Required as a proof that we already received the remote reset terms):
    let reset_token = match &friend_report.channel_status {
        ChannelStatusReport::Consistent(_) => return Err(ConfigError::ChannelNotInconsistent),
        ChannelStatusReport::Inconsistent(channel_inconsistent_report) => {
            if let Some(remote_reset_terms) = &channel_inconsistent_report.opt_remote_reset_terms {
                &remote_reset_terms.reset_token
            } else {
                return Err(ConfigError::UnknownRemoteResetTerms);
            }
        }
    };

    await!(app_config.reset_friend_channel(friend_public_key.clone(), reset_token.clone()))
        .map_err(|_| ConfigError::AppConfigError)
}

pub async fn config<'a>(
    matches: &'a ArgMatches<'a>,
    mut node_connection: NodeConnection,
) -> Result<(), ConfigError> {
    let app_config = node_connection
        .config()
        .ok_or(ConfigError::NoPermissions)?
        .clone();

    // Obtain current report:
    let app_report = node_connection.report();
    let (node_report, _incoming_mutations) =
        await!(app_report.incoming_reports()).map_err(|_| ConfigError::GetReportError)?;

    match matches.subcommand() {
        ("add-relay", Some(matches)) => await!(config_add_relay(matches, app_config, node_report))?,
        ("remove-relay", Some(matches)) => {
            await!(config_remove_relay(matches, app_config, node_report))?
        }
        ("add-index", Some(matches)) => await!(config_add_index(matches, app_config, node_report))?,
        ("remove-index", Some(matches)) => {
            await!(config_remove_index(matches, app_config, node_report))?
        }
        ("add-friend", Some(matches)) => {
            await!(config_add_friend(matches, app_config, node_report))?
        }
        ("set-friend-relays", Some(matches)) => {
            await!(config_set_friend_relays(matches, app_config, node_report))?
        }
        ("remove-friend", Some(matches)) => {
            await!(config_remove_friend(matches, app_config, node_report))?
        }
        ("enable-friend", Some(matches)) => {
            await!(config_enable_friend(matches, app_config, node_report))?
        }
        ("disable-friend", Some(matches)) => {
            await!(config_disable_friend(matches, app_config, node_report))?
        }
        ("open-friend", Some(matches)) => {
            await!(config_open_friend(matches, app_config, node_report))?
        }
        ("close-friend", Some(matches)) => {
            await!(config_close_friend(matches, app_config, node_report))?
        }
        ("set-friend-max-debt", Some(matches)) => {
            await!(config_set_friend_max_debt(matches, app_config, node_report))?
        }
        ("reset-friend", Some(matches)) => {
            await!(config_reset_friend(matches, app_config, node_report))?
        }
        _ => unreachable!(),
    }

    Ok(())
}
