use std::path::PathBuf;

use structopt::StructOpt;

use app::report::{ChannelStatusReport, NodeReport};
use app::{
    load_friend_from_file, load_index_server_from_file, load_relay_from_file, AppConfig, AppConn,
    NamedIndexServerAddress, NamedRelayAddress, Rate,
};

use crate::utils::friend_public_key_by_name;

/// Add a relay
#[derive(Clone, Debug, StructOpt)]
pub struct AddRelayCmd {
    /// Path of relay file
    #[structopt(parse(from_os_str), long = "relay", short = "r")]
    pub relay_file: PathBuf,
    /// Assigned relay name (You can pick any name)
    #[structopt(long = "name", short = "n")]
    pub relay_name: String,
}

/// Remove relay
#[derive(Clone, Debug, StructOpt)]
pub struct RemoveRelayCmd {
    /// Relay name to remove
    #[structopt(long = "name", short = "n")]
    pub relay_name: String,
}

/// Add index
#[derive(Clone, Debug, StructOpt)]
pub struct AddIndexCmd {
    /// Path of index file
    #[structopt(parse(from_os_str), long = "index", short = "i")]
    pub index_file: PathBuf,
    /// Assigned index name (You can pick any name)
    #[structopt(long = "name", short = "n")]
    pub index_name: String,
}

/// Remove index
#[derive(Clone, Debug, StructOpt)]
pub struct RemoveIndexCmd {
    /// Index name to remove
    #[structopt(long = "name", short = "n")]
    pub index_name: String,
}

/// Add friend
#[derive(Clone, Debug, StructOpt)]
pub struct AddFriendCmd {
    /// Path of friend file
    #[structopt(parse(from_os_str), long = "friend", short = "f")]
    pub friend_file: PathBuf,
    /// Assigned friend name (You can pick any name)
    #[structopt(long = "name", short = "n")]
    pub friend_name: String,
    /// Initial balance with friend
    #[structopt(long = "balance")]
    pub balance: i128,
}

/// Set friend relays
#[derive(Clone, Debug, StructOpt)]
pub struct SetFriendRelaysCmd {
    /// Path of friend file
    #[structopt(parse(from_os_str), long = "friend", short = "f")]
    pub friend_file: PathBuf,
    /// Friend name (Must be an existing friend)
    #[structopt(long = "name", short = "n")]
    pub friend_name: String,
}

/// Remove friend
#[derive(Clone, Debug, StructOpt)]
pub struct RemoveFriendCmd {
    /// Friend name to remove
    #[structopt(long = "name", short = "n")]
    pub friend_name: String,
}

/// Enable friend
#[derive(Clone, Debug, StructOpt)]
pub struct EnableFriendCmd {
    /// Friend name to enable
    #[structopt(long = "name", short = "n")]
    pub friend_name: String,
}

/// Disable friend
#[derive(Clone, Debug, StructOpt)]
pub struct DisableFriendCmd {
    /// Friend name to disable
    #[structopt(long = "name", short = "n")]
    pub friend_name: String,
}

/// Enable forwarding of payment requests from friend to us
#[derive(Clone, Debug, StructOpt)]
pub struct OpenFriendCmd {
    /// Friend name to open
    #[structopt(long = "name", short = "n")]
    pub friend_name: String,
}

/// Disable forwarding of payment requests from friend to us
#[derive(Clone, Debug, StructOpt)]
pub struct CloseFriendCmd {
    /// Friend name to close
    #[structopt(long = "name", short = "n")]
    pub friend_name: String,
}

/// Set friend's maximum allowed debt
/// If you lose this friend, you can lose this amount of credits.
#[derive(Clone, Debug, StructOpt)]
pub struct SetFriendMaxDebtCmd {
    /// Friend name
    #[structopt(long = "name", short = "n")]
    pub friend_name: String,
    /// Max debt allowed for friend
    #[structopt(long = "mdebt", short = "m")]
    pub max_debt: u128,
}

/// Set friend's maximum allowed debt
/// If you lose this friend, you can lose this amount of credits.
#[derive(Clone, Debug, StructOpt)]
pub struct SetFriendRateCmd {
    /// Friend name
    #[structopt(long = "name", short = "n")]
    pub friend_name: String,
    /// Multiplier (fee = x * mul + add)
    #[structopt(long = "mul", short = "m")]
    pub mul: u32,
    /// Adder (fee = x * mul + add)
    #[structopt(long = "add", short = "a")]
    pub add: u32,
}

/// Reset mutual credit with friend according to friend's terms.
#[derive(Clone, Debug, StructOpt)]
pub struct ResetFriendCmd {
    /// Friend name to reset
    #[structopt(long = "name", short = "n")]
    pub friend_name: String,
}

#[derive(Clone, Debug, StructOpt)]
pub enum ConfigCmd {
    /// Add a relay server
    #[structopt(name = "add-relay")]
    AddRelay(AddRelayCmd),
    /// Remove a relay server
    #[structopt(name = "remove-relay")]
    RemoveRelay(RemoveRelayCmd),
    /// Add an index server
    #[structopt(name = "add-index")]
    AddIndex(AddIndexCmd),
    /// Remove an index server
    #[structopt(name = "remove-index")]
    RemoveIndex(RemoveIndexCmd),
    /// Add a new friend
    #[structopt(name = "add-friend")]
    AddFriend(AddFriendCmd),
    /// Update friend's relays
    #[structopt(name = "set-friend-relays")]
    SetFriendRelays(SetFriendRelaysCmd),
    /// Remove a friend
    #[structopt(name = "remove-friend")]
    RemoveFriend(RemoveFriendCmd),
    /// Enable a friend
    #[structopt(name = "enable-friend")]
    EnableFriend(EnableFriendCmd),
    /// Disable a friend
    #[structopt(name = "disable-friend")]
    DisableFriend(DisableFriendCmd),
    /// Open requests from friend
    #[structopt(name = "open-friend")]
    OpenFriend(OpenFriendCmd),
    /// Close requests from friend
    #[structopt(name = "close-friend")]
    CloseFriend(CloseFriendCmd),
    /// Set friend's max debt
    #[structopt(name = "set-friend-max-debt")]
    SetFriendMaxDebt(SetFriendMaxDebtCmd),
    /// Set friend's rate: How much we charge for forwarding this friend's transactions to other
    /// friends?
    #[structopt(name = "set-friend-rate")]
    SetFriendRate(SetFriendRateCmd),
    /// Reset mutual credit with a friend according to friend's terms
    #[structopt(name = "reset-friend")]
    ResetFriend(ResetFriendCmd),
}

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

async fn config_add_relay(
    add_relay_cmd: AddRelayCmd,
    mut app_config: AppConfig,
    node_report: NodeReport,
) -> Result<(), ConfigError> {
    for named_relay_address in node_report.funder_report.relays {
        if named_relay_address.name == add_relay_cmd.relay_name {
            return Err(ConfigError::RelayNameAlreadyExists);
        }
    }

    if !add_relay_cmd.relay_file.exists() {
        return Err(ConfigError::RelayFileNotFound);
    }

    let relay_address = load_relay_from_file(&add_relay_cmd.relay_file)
        .map_err(|_| ConfigError::LoadRelayFromFileError)?;

    let named_relay_address = NamedRelayAddress {
        public_key: relay_address.public_key,
        address: relay_address.address,
        name: add_relay_cmd.relay_name.to_owned(),
    };

    await!(app_config.add_relay(named_relay_address)).map_err(|_| ConfigError::AppConfigError)
}

async fn config_remove_relay(
    remove_relay_cmd: RemoveRelayCmd,
    mut app_config: AppConfig,
    node_report: NodeReport,
) -> Result<(), ConfigError> {
    let mut opt_relay_public_key = None;
    for named_relay_address in node_report.funder_report.relays {
        if named_relay_address.name == remove_relay_cmd.relay_name {
            opt_relay_public_key = Some(named_relay_address.public_key.clone());
        }
    }

    let relay_public_key = opt_relay_public_key.ok_or(ConfigError::RelayNameNotFound)?;

    await!(app_config.remove_relay(relay_public_key)).map_err(|_| ConfigError::AppConfigError)
}

async fn config_add_index(
    add_index_cmd: AddIndexCmd,
    mut app_config: AppConfig,
    node_report: NodeReport,
) -> Result<(), ConfigError> {
    let AddIndexCmd {
        index_file,
        index_name,
    } = add_index_cmd;

    for named_index_server_address in node_report.index_client_report.index_servers {
        if named_index_server_address.name == index_name {
            return Err(ConfigError::IndexNameAlreadyExists);
        }
    }

    if !index_file.exists() {
        return Err(ConfigError::IndexFileNotFound);
    }

    let index_server_address = load_index_server_from_file(&index_file)
        .map_err(|_| ConfigError::LoadIndexFromFileError)?;

    let named_index_server_address = NamedIndexServerAddress {
        public_key: index_server_address.public_key,
        address: index_server_address.address,
        name: index_name.to_owned(),
    };

    await!(app_config.add_index_server(named_index_server_address))
        .map_err(|_| ConfigError::AppConfigError)
}

async fn config_remove_index(
    remove_index_cmd: RemoveIndexCmd,
    mut app_config: AppConfig,
    node_report: NodeReport,
) -> Result<(), ConfigError> {
    let mut opt_index_public_key = None;
    for named_index_server_address in node_report.index_client_report.index_servers {
        if named_index_server_address.name == remove_index_cmd.index_name {
            opt_index_public_key = Some(named_index_server_address.public_key.clone());
        }
    }

    let index_public_key = opt_index_public_key.ok_or(ConfigError::RelayNameNotFound)?;

    await!(app_config.remove_index_server(index_public_key))
        .map_err(|_| ConfigError::AppConfigError)
}

async fn config_add_friend(
    add_friend_cmd: AddFriendCmd,
    mut app_config: AppConfig,
    node_report: NodeReport,
) -> Result<(), ConfigError> {
    let AddFriendCmd {
        friend_file,
        friend_name,
        balance,
    } = add_friend_cmd;

    for (_friend_public_key, friend_report) in node_report.funder_report.friends {
        if friend_report.name == friend_name {
            return Err(ConfigError::FriendNameAlreadyExists);
        }
    }

    if !friend_file.exists() {
        return Err(ConfigError::FriendFileNotFound);
    }

    let friend_address =
        load_friend_from_file(&friend_file).map_err(|_| ConfigError::LoadFriendFromFileError)?;

    await!(app_config.add_friend(
        friend_address.public_key,
        friend_address.relays,
        friend_name.to_owned(),
        balance
    ))
    .map_err(|_| ConfigError::AppConfigError)?;
    Ok(())
}

async fn config_set_friend_relays(
    set_friend_relays_cmd: SetFriendRelaysCmd,
    mut app_config: AppConfig,
    node_report: NodeReport,
) -> Result<(), ConfigError> {
    let SetFriendRelaysCmd {
        friend_file,
        friend_name,
    } = set_friend_relays_cmd;

    let friend_public_key = friend_public_key_by_name(&node_report, &friend_name)
        .ok_or(ConfigError::FriendNameNotFound)?
        .clone();

    if !friend_file.exists() {
        return Err(ConfigError::FriendFileNotFound);
    }

    let friend_address =
        load_friend_from_file(&friend_file).map_err(|_| ConfigError::LoadFriendFromFileError)?;

    // Just in case, make sure that the the friend we know with this name
    // has the same public key as inside the provided file.
    if friend_address.public_key != friend_public_key {
        return Err(ConfigError::FriendPublicKeyMismatch);
    }

    await!(app_config.set_friend_relays(friend_public_key, friend_address.relays))
        .map_err(|_| ConfigError::AppConfigError)?;

    Ok(())
}

async fn config_remove_friend(
    remove_friend_cmd: RemoveFriendCmd,
    mut app_config: AppConfig,
    node_report: NodeReport,
) -> Result<(), ConfigError> {
    let friend_public_key = friend_public_key_by_name(&node_report, &remove_friend_cmd.friend_name)
        .ok_or(ConfigError::FriendNameNotFound)?
        .clone();

    await!(app_config.remove_friend(friend_public_key)).map_err(|_| ConfigError::AppConfigError)
}

async fn config_enable_friend(
    enable_friend_cmd: EnableFriendCmd,
    mut app_config: AppConfig,
    node_report: NodeReport,
) -> Result<(), ConfigError> {
    let friend_public_key = friend_public_key_by_name(&node_report, &enable_friend_cmd.friend_name)
        .ok_or(ConfigError::FriendNameNotFound)?
        .clone();

    await!(app_config.enable_friend(friend_public_key)).map_err(|_| ConfigError::AppConfigError)
}

async fn config_disable_friend(
    disable_friend_cmd: DisableFriendCmd,
    mut app_config: AppConfig,
    node_report: NodeReport,
) -> Result<(), ConfigError> {
    let friend_public_key =
        friend_public_key_by_name(&node_report, &disable_friend_cmd.friend_name)
            .ok_or(ConfigError::FriendNameNotFound)?
            .clone();

    await!(app_config.disable_friend(friend_public_key)).map_err(|_| ConfigError::AppConfigError)
}

async fn config_open_friend(
    open_friend_cmd: OpenFriendCmd,
    mut app_config: AppConfig,
    node_report: NodeReport,
) -> Result<(), ConfigError> {
    let friend_public_key = friend_public_key_by_name(&node_report, &open_friend_cmd.friend_name)
        .ok_or(ConfigError::FriendNameNotFound)?
        .clone();

    await!(app_config.open_friend(friend_public_key)).map_err(|_| ConfigError::AppConfigError)
}

async fn config_close_friend(
    close_friend_cmd: CloseFriendCmd,
    mut app_config: AppConfig,
    node_report: NodeReport,
) -> Result<(), ConfigError> {
    let friend_public_key = friend_public_key_by_name(&node_report, &close_friend_cmd.friend_name)
        .ok_or(ConfigError::FriendNameNotFound)?
        .clone();

    await!(app_config.close_friend(friend_public_key)).map_err(|_| ConfigError::AppConfigError)
}

async fn config_set_friend_max_debt(
    set_friend_max_debt_cmd: SetFriendMaxDebtCmd,
    mut app_config: AppConfig,
    node_report: NodeReport,
) -> Result<(), ConfigError> {
    let SetFriendMaxDebtCmd {
        friend_name,
        max_debt,
    } = set_friend_max_debt_cmd;

    let friend_public_key = friend_public_key_by_name(&node_report, &friend_name)
        .ok_or(ConfigError::FriendNameNotFound)?
        .clone();

    await!(app_config.set_friend_remote_max_debt(friend_public_key, max_debt))
        .map_err(|_| ConfigError::AppConfigError)
}

async fn config_set_friend_rate(
    set_friend_rate_cmd: SetFriendRateCmd,
    mut app_config: AppConfig,
    node_report: NodeReport,
) -> Result<(), ConfigError> {
    let SetFriendRateCmd {
        friend_name,
        mul,
        add,
    } = set_friend_rate_cmd;

    let friend_public_key = friend_public_key_by_name(&node_report, &friend_name)
        .ok_or(ConfigError::FriendNameNotFound)?
        .clone();

    let rate = Rate { mul, add };

    await!(app_config.set_friend_rate(friend_public_key, rate))
        .map_err(|_| ConfigError::AppConfigError)
}

async fn config_reset_friend(
    reset_friend_cmd: ResetFriendCmd,
    mut app_config: AppConfig,
    node_report: NodeReport,
) -> Result<(), ConfigError> {
    let mut opt_friend_pk_report = None;
    for (friend_public_key, friend_report) in &node_report.funder_report.friends {
        if friend_report.name == reset_friend_cmd.friend_name {
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

pub async fn config(config_cmd: ConfigCmd, mut app_conn: AppConn) -> Result<(), ConfigError> {
    let app_config = app_conn.config().ok_or(ConfigError::NoPermissions)?.clone();

    // Obtain current report:
    let node_report = {
        // other vars should be dropped to prevent deadlock
        let app_report = app_conn.report();
        let (node_report, _incoming_mutations) =
            await!(app_report.incoming_reports()).map_err(|_| ConfigError::GetReportError)?;
        node_report
    };

    match config_cmd {
        ConfigCmd::AddRelay(add_relay_cmd) => {
            await!(config_add_relay(add_relay_cmd, app_config, node_report))?
        }
        ConfigCmd::RemoveRelay(remove_relay_cmd) => await!(config_remove_relay(
            remove_relay_cmd,
            app_config,
            node_report
        ))?,
        ConfigCmd::AddIndex(add_index_cmd) => {
            await!(config_add_index(add_index_cmd, app_config, node_report))?
        }
        ConfigCmd::RemoveIndex(remove_index_cmd) => await!(config_remove_index(
            remove_index_cmd,
            app_config,
            node_report
        ))?,
        ConfigCmd::AddFriend(add_friend_cmd) => {
            await!(config_add_friend(add_friend_cmd, app_config, node_report))?
        }
        ConfigCmd::SetFriendRelays(set_friend_relays_cmd) => await!(config_set_friend_relays(
            set_friend_relays_cmd,
            app_config,
            node_report
        ))?,
        ConfigCmd::RemoveFriend(remove_friend_cmd) => await!(config_remove_friend(
            remove_friend_cmd,
            app_config,
            node_report
        ))?,
        ConfigCmd::EnableFriend(enable_friend_cmd) => await!(config_enable_friend(
            enable_friend_cmd,
            app_config,
            node_report
        ))?,
        ConfigCmd::DisableFriend(disable_friend_cmd) => await!(config_disable_friend(
            disable_friend_cmd,
            app_config,
            node_report
        ))?,
        ConfigCmd::OpenFriend(open_friend_cmd) => {
            await!(config_open_friend(open_friend_cmd, app_config, node_report))?
        }
        ConfigCmd::CloseFriend(close_friend_cmd) => await!(config_close_friend(
            close_friend_cmd,
            app_config,
            node_report
        ))?,
        ConfigCmd::SetFriendMaxDebt(set_friend_max_debt_cmd) => await!(
            config_set_friend_max_debt(set_friend_max_debt_cmd, app_config, node_report)
        )?,
        ConfigCmd::SetFriendRate(set_friend_rate_cmd) => await!(config_set_friend_rate(
            set_friend_rate_cmd,
            app_config,
            node_report
        ))?,
        ConfigCmd::ResetFriend(reset_friend_cmd) => await!(config_reset_friend(
            reset_friend_cmd,
            app_config,
            node_report
        ))?,
    }

    Ok(())
}
