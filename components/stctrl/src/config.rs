use std::convert::TryFrom;
use std::fs;
use std::path::PathBuf;

use futures::sink::SinkExt;
use futures::stream::StreamExt;

use structopt::StructOpt;

use derive_more::From;

use app::common::{Currency, NamedIndexServerAddress, NamedRelayAddress, Rate, RelayAddress};
use app::conn::{ConnPairApp, self, AppToAppServer, AppServerToApp, AppRequest};
use app::report::{ChannelStatusReport, NodeReport};
use app::gen::gen_uid;

use app::file::{FriendFile, IndexServerFile, RelayAddressFile};
use app::ser_string::{deserialize_from_string, StringSerdeError};


use crate::utils::friend_public_key_by_name;

/// Add a relay
#[derive(Clone, Debug, StructOpt)]
pub struct AddRelayCmd {
    /// Path of relay file
    #[structopt(parse(from_os_str), long = "relay", short = "r")]
    pub relay_path: PathBuf,
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
    pub index_path: PathBuf,
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
    pub friend_path: PathBuf,
    /// Assigned friend name (You can pick any name)
    #[structopt(long = "name", short = "n")]
    pub friend_name: String,
}

/// Set friend relays
#[derive(Clone, Debug, StructOpt)]
pub struct SetFriendRelaysCmd {
    /// Path of friend file
    #[structopt(parse(from_os_str), long = "friend", short = "f")]
    pub friend_path: PathBuf,
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
pub struct OpenFriendCurrencyCmd {
    /// Friend name to open
    #[structopt(long = "name", short = "n")]
    pub friend_name: String,
    /// Currency to open
    #[structopt(long = "currency", short = "c")]
    pub currency_name: String,
}

/// Disable forwarding of payment requests from friend to us
#[derive(Clone, Debug, StructOpt)]
pub struct CloseFriendCurrencyCmd {
    /// Friend name to close
    #[structopt(long = "name", short = "n")]
    pub friend_name: String,
    /// Currency to close
    #[structopt(long = "currency", short = "c")]
    pub currency_name: String,
}

/// Set friend's maximum allowed debt
/// If you lose this friend, you can lose this amount of credits.
#[derive(Clone, Debug, StructOpt)]
pub struct SetFriendCurrencyMaxDebtCmd {
    /// Friend name
    #[structopt(long = "name", short = "n")]
    pub friend_name: String,
    /// Currency to set remote max debt
    #[structopt(long = "currency", short = "c")]
    pub currency_name: String,
    /// Max debt allowed for friend
    #[structopt(long = "mdebt", short = "m")]
    pub max_debt: u128,
}

/// Set friend's maximum allowed debt
/// If you lose this friend, you can lose this amount of credits.
#[derive(Clone, Debug, StructOpt)]
pub struct SetFriendCurrencyRateCmd {
    /// Friend name
    #[structopt(long = "name", short = "n")]
    pub friend_name: String,
    /// Currency to set remote max debt
    #[structopt(long = "currency", short = "c")]
    pub currency_name: String,
    /// Multiplier (fee = x * mul + add)
    #[structopt(long = "mul", short = "m")]
    pub mul: u32,
    /// Adder (fee = x * mul + add)
    #[structopt(long = "add", short = "a")]
    pub add: u32,
}

/// Remove a currency from the set of currencies we are willing to trade
/// with a remote friend.
/// This operation will succeed only if we do not already have an active channel trading this
/// currency.
/// If we already have an open active channel with this currency, the currency will not
/// be removed.
#[derive(Clone, Debug, StructOpt)]
pub struct RemoveFriendCurrencyCmd {
    /// Friend name
    #[structopt(long = "name", short = "n")]
    pub friend_name: String,
    /// Currency to set remote max debt
    #[structopt(long = "currency", short = "c")]
    pub currency_name: String,
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
    #[structopt(name = "open-currency")]
    OpenFriendCurrency(OpenFriendCurrencyCmd),
    /// Close requests from friend
    #[structopt(name = "close-currency")]
    CloseFriendCurrency(CloseFriendCurrencyCmd),
    /// Set friend's max debt
    #[structopt(name = "set-currency-max-debt")]
    SetFriendCurrencyMaxDebt(SetFriendCurrencyMaxDebtCmd),
    /// Set friend's rate: How much we charge for forwarding this friend's transactions to other
    /// friends?
    #[structopt(name = "set-currency-rate")]
    SetFriendCurrencyRate(SetFriendCurrencyRateCmd),
    /// Remove a currency from the set of currencies we are willing to trade with a friend.
    #[structopt(name = "remove-currency")]
    RemoveFriendCurrency(RemoveFriendCurrencyCmd),
    /// Reset mutual credit with a friend according to friend's terms
    #[structopt(name = "reset-friend")]
    ResetFriend(ResetFriendCmd),
}

#[derive(Debug, From)]
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
    IoError(std::io::Error),
    StringSerdeError(StringSerdeError),
    InvalidCurrencyName,
}

async fn config_request(conn_pair: &mut ConnPairApp, app_request: AppRequest) -> Result<(), ConfigError> {
    let app_request_id = gen_uid();
    let app_to_app_server = AppToAppServer {
        app_request_id: app_request_id.clone(),
        app_request,
    };
    conn_pair.sender.send(app_to_app_server).await.map_err(|_| ConfigError::AppConfigError);

    // Wait until we get an ack for our request:
    while let Some(app_server_to_app) = conn_pair.receiver.next().await {
        if let AppServerToApp::ReportMutations(report_mutations) = app_server_to_app {
            if let Some(cur_app_request_id) = report_mutations.opt_app_request_id {
                if cur_app_request_id == app_request_id {
                    return Ok(())
                }
            }
        }
    }

    return Err(ConfigError::AppConfigError);
}

async fn config_add_relay(
    add_relay_cmd: AddRelayCmd,
    mut conn_pair: ConnPairApp,
    node_report: &NodeReport,
) -> Result<(), ConfigError> {
    for named_relay_address in &node_report.funder_report.relays {
        if named_relay_address.name == add_relay_cmd.relay_name {
            return Err(ConfigError::RelayNameAlreadyExists);
        }
    }

    if !add_relay_cmd.relay_path.exists() {
        return Err(ConfigError::RelayFileNotFound);
    }

    let relay_file: RelayAddressFile =
        deserialize_from_string(&fs::read_to_string(&add_relay_cmd.relay_path)?)?;

    let named_relay_address = NamedRelayAddress {
        public_key: relay_file.public_key,
        address: relay_file.address,
        name: add_relay_cmd.relay_name.to_owned(),
    };

    let app_request = conn::config::add_relay(named_relay_address);
    config_request(&mut conn_pair, app_request).await
}

async fn config_remove_relay(
    remove_relay_cmd: RemoveRelayCmd,
    mut conn_pair: ConnPairApp,
    node_report: &NodeReport,
) -> Result<(), ConfigError> {
    let mut opt_relay_public_key = None;
    for named_relay_address in &node_report.funder_report.relays {
        if named_relay_address.name == remove_relay_cmd.relay_name {
            opt_relay_public_key = Some(named_relay_address.public_key.clone());
        }
    }

    let relay_public_key = opt_relay_public_key.ok_or(ConfigError::RelayNameNotFound)?;

    let app_request = conn::config::remove_relay(relay_public_key);
    config_request(&mut conn_pair, app_request).await
}

async fn config_add_index(
    add_index_cmd: AddIndexCmd,
    mut conn_pair: ConnPairApp,
    node_report: &NodeReport,
) -> Result<(), ConfigError> {
    let AddIndexCmd {
        index_path,
        index_name,
    } = add_index_cmd;

    for named_index_server_address in &node_report.index_client_report.index_servers {
        if named_index_server_address.name == index_name {
            return Err(ConfigError::IndexNameAlreadyExists);
        }
    }

    if !index_path.exists() {
        return Err(ConfigError::IndexFileNotFound);
    }

    let index_server_file: IndexServerFile =
        deserialize_from_string(&fs::read_to_string(&index_path)?)?;

    let named_index_server_address = NamedIndexServerAddress {
        public_key: index_server_file.public_key,
        address: index_server_file.address,
        name: index_name.to_owned(),
    };

    let app_request = conn::config::add_index_server(named_index_server_address);
    config_request(&mut conn_pair, app_request).await
}

async fn config_remove_index(
    remove_index_cmd: RemoveIndexCmd,
    mut conn_pair: ConnPairApp,
    node_report: &NodeReport,
) -> Result<(), ConfigError> {
    let mut opt_index_public_key = None;
    for named_index_server_address in &node_report.index_client_report.index_servers {
        if named_index_server_address.name == remove_index_cmd.index_name {
            opt_index_public_key = Some(named_index_server_address.public_key.clone());
        }
    }

    let index_public_key = opt_index_public_key.ok_or(ConfigError::RelayNameNotFound)?;

    let app_request = conn::config::remove_index_server(index_public_key);
    config_request(&mut conn_pair, app_request).await
}

async fn config_add_friend(
    add_friend_cmd: AddFriendCmd,
    mut conn_pair: ConnPairApp,
    node_report: &NodeReport,
) -> Result<(), ConfigError> {
    let AddFriendCmd {
        friend_path,
        friend_name,
    } = add_friend_cmd;

    for (_friend_public_key, friend_report) in &node_report.funder_report.friends {
        if friend_report.name == friend_name {
            return Err(ConfigError::FriendNameAlreadyExists);
        }
    }

    if !friend_path.exists() {
        return Err(ConfigError::FriendFileNotFound);
    }

    let friend_file: FriendFile = deserialize_from_string(&fs::read_to_string(&friend_path)?)?;

    let app_request = conn::config::add_friend(
            friend_file.public_key,
            friend_file
                .relays
                .into_iter()
                .map(RelayAddress::from)
                .collect(),
            friend_name.to_owned());
    config_request(&mut conn_pair, app_request).await
}

async fn config_set_friend_relays(
    set_friend_relays_cmd: SetFriendRelaysCmd,
    mut conn_pair: ConnPairApp,
    node_report: &NodeReport,
) -> Result<(), ConfigError> {
    let SetFriendRelaysCmd {
        friend_path,
        friend_name,
    } = set_friend_relays_cmd;

    let friend_public_key = friend_public_key_by_name(&node_report, &friend_name)
        .ok_or(ConfigError::FriendNameNotFound)?
        .clone();

    if !friend_path.exists() {
        return Err(ConfigError::FriendFileNotFound);
    }

    let friend_file: FriendFile = deserialize_from_string(&fs::read_to_string(&friend_path)?)?;

    // Just in case, make sure that the the friend we know with this name
    // has the same public key as inside the provided file.
    if friend_file.public_key != friend_public_key {
        return Err(ConfigError::FriendPublicKeyMismatch);
    }
    
    let app_request = conn::config::set_friend_relays(
            friend_public_key,
            friend_file
                .relays
                .into_iter()
                .map(RelayAddress::from)
                .collect());
    config_request(&mut conn_pair, app_request).await
}

async fn config_remove_friend(
    remove_friend_cmd: RemoveFriendCmd,
    mut conn_pair: ConnPairApp,
    node_report: &NodeReport,
) -> Result<(), ConfigError> {
    let friend_public_key = friend_public_key_by_name(&node_report, &remove_friend_cmd.friend_name)
        .ok_or(ConfigError::FriendNameNotFound)?
        .clone();

    let app_request = conn::config::remove_friend(friend_public_key);
    config_request(&mut conn_pair, app_request).await
}

async fn config_enable_friend(
    enable_friend_cmd: EnableFriendCmd,
    mut conn_pair: ConnPairApp,
    node_report: &NodeReport,
) -> Result<(), ConfigError> {
    let friend_public_key = friend_public_key_by_name(&node_report, &enable_friend_cmd.friend_name)
        .ok_or(ConfigError::FriendNameNotFound)?
        .clone();

    let app_request = conn::config::enable_friend(friend_public_key);
    config_request(&mut conn_pair, app_request).await
}

async fn config_disable_friend(
    disable_friend_cmd: DisableFriendCmd,
    mut conn_pair: ConnPairApp,
    node_report: &NodeReport,
) -> Result<(), ConfigError> {
    let friend_public_key =
        friend_public_key_by_name(&node_report, &disable_friend_cmd.friend_name)
            .ok_or(ConfigError::FriendNameNotFound)?
            .clone();

    let app_request = conn::config::disable_friend(friend_public_key);
    config_request(&mut conn_pair, app_request).await
}

async fn config_open_friend_currency(
    open_friend_currency_cmd: OpenFriendCurrencyCmd,
    mut conn_pair: ConnPairApp,
    node_report: &NodeReport,
) -> Result<(), ConfigError> {
    let friend_public_key =
        friend_public_key_by_name(&node_report, &open_friend_currency_cmd.friend_name)
            .ok_or(ConfigError::FriendNameNotFound)?
            .clone();

    let currency = Currency::try_from(open_friend_currency_cmd.currency_name)
        .map_err(|_| ConfigError::InvalidCurrencyName)?;

    let app_request = conn::config::open_friend_currency(friend_public_key, currency);
    config_request(&mut conn_pair, app_request).await
}

async fn config_close_friend_currency(
    close_friend_currency_cmd: CloseFriendCurrencyCmd,
    mut conn_pair: ConnPairApp,
    node_report: &NodeReport,
) -> Result<(), ConfigError> {
    let friend_public_key =
        friend_public_key_by_name(&node_report, &close_friend_currency_cmd.friend_name)
            .ok_or(ConfigError::FriendNameNotFound)?
            .clone();

    let currency = Currency::try_from(close_friend_currency_cmd.currency_name)
        .map_err(|_| ConfigError::InvalidCurrencyName)?;

    let app_request = conn::config::close_friend_currency(friend_public_key, currency);
    config_request(&mut conn_pair, app_request).await
}

async fn config_set_friend_currency_max_debt(
    set_friend_currency_max_debt_cmd: SetFriendCurrencyMaxDebtCmd,
    mut conn_pair: ConnPairApp,
    node_report: &NodeReport,
) -> Result<(), ConfigError> {
    let SetFriendCurrencyMaxDebtCmd {
        friend_name,
        currency_name,
        max_debt,
    } = set_friend_currency_max_debt_cmd;

    let friend_public_key = friend_public_key_by_name(&node_report, &friend_name)
        .ok_or(ConfigError::FriendNameNotFound)?
        .clone();

    let currency =
        Currency::try_from(currency_name).map_err(|_| ConfigError::InvalidCurrencyName)?;

    let app_request = conn::config::set_friend_currency_max_debt(friend_public_key, currency, max_debt);
    config_request(&mut conn_pair, app_request).await
}

async fn config_set_friend_currency_rate(
    set_friend_currency_rate_cmd: SetFriendCurrencyRateCmd,
    mut conn_pair: ConnPairApp,
    node_report: &NodeReport,
) -> Result<(), ConfigError> {
    let SetFriendCurrencyRateCmd {
        friend_name,
        currency_name,
        mul,
        add,
    } = set_friend_currency_rate_cmd;

    let friend_public_key = friend_public_key_by_name(&node_report, &friend_name)
        .ok_or(ConfigError::FriendNameNotFound)?
        .clone();

    let currency =
        Currency::try_from(currency_name).map_err(|_| ConfigError::InvalidCurrencyName)?;

    let rate = Rate { mul, add };

    let app_request = conn::config::set_friend_currency_rate(friend_public_key, currency, rate);
    config_request(&mut conn_pair, app_request).await
}

async fn config_remove_friend_currency(
    remove_friend_currency_cmd: RemoveFriendCurrencyCmd,
    mut conn_pair: ConnPairApp,
    node_report: &NodeReport,
) -> Result<(), ConfigError> {
    let RemoveFriendCurrencyCmd {
        friend_name,
        currency_name,
    } = remove_friend_currency_cmd;

    let friend_public_key = friend_public_key_by_name(&node_report, &friend_name)
        .ok_or(ConfigError::FriendNameNotFound)?
        .clone();

    let currency =
        Currency::try_from(currency_name).map_err(|_| ConfigError::InvalidCurrencyName)?;

    let app_request = conn::config::remove_friend_currency(friend_public_key, currency);
    config_request(&mut conn_pair, app_request).await
}

async fn config_reset_friend(
    reset_friend_cmd: ResetFriendCmd,
    mut conn_pair: ConnPairApp,
    node_report: &NodeReport,
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

    let app_request = conn::config::reset_friend_channel(friend_public_key.clone(), reset_token.clone());
    config_request(&mut conn_pair, app_request).await
}

pub async fn config(config_cmd: ConfigCmd, node_report: &NodeReport, conn_pair: ConnPairApp) -> Result<(), ConfigError> {
    match config_cmd {
        ConfigCmd::AddRelay(add_relay_cmd) => {
            config_add_relay(add_relay_cmd, conn_pair, node_report).await?
        }
        ConfigCmd::RemoveRelay(remove_relay_cmd) => {
            config_remove_relay(remove_relay_cmd, conn_pair, node_report).await?
        }
        ConfigCmd::AddIndex(add_index_cmd) => {
            config_add_index(add_index_cmd, conn_pair, node_report).await?
        }
        ConfigCmd::RemoveIndex(remove_index_cmd) => {
            config_remove_index(remove_index_cmd, conn_pair, node_report).await?
        }
        ConfigCmd::AddFriend(add_friend_cmd) => {
            config_add_friend(add_friend_cmd, conn_pair, node_report).await?
        }
        ConfigCmd::SetFriendRelays(set_friend_relays_cmd) => {
            config_set_friend_relays(set_friend_relays_cmd, conn_pair, node_report).await?
        }
        ConfigCmd::RemoveFriend(remove_friend_cmd) => {
            config_remove_friend(remove_friend_cmd, conn_pair, node_report).await?
        }
        ConfigCmd::EnableFriend(enable_friend_cmd) => {
            config_enable_friend(enable_friend_cmd, conn_pair, node_report).await?
        }
        ConfigCmd::DisableFriend(disable_friend_cmd) => {
            config_disable_friend(disable_friend_cmd, conn_pair, node_report).await?
        }
        ConfigCmd::OpenFriendCurrency(open_friend_currency_cmd) => {
            config_open_friend_currency(open_friend_currency_cmd, conn_pair, node_report).await?
        }
        ConfigCmd::CloseFriendCurrency(close_friend_currency_cmd) => {
            config_close_friend_currency(close_friend_currency_cmd, conn_pair, node_report).await?
        }
        ConfigCmd::SetFriendCurrencyMaxDebt(set_friend_currency_max_debt_cmd) => {
            config_set_friend_currency_max_debt(
                set_friend_currency_max_debt_cmd,
                conn_pair,
                node_report,
            )
            .await?
        }
        ConfigCmd::SetFriendCurrencyRate(set_friend_currency_rate_cmd) => {
            config_set_friend_currency_rate(set_friend_currency_rate_cmd, conn_pair, node_report)
                .await?
        }
        ConfigCmd::RemoveFriendCurrency(remove_friend_currency_cmd) => {
            config_remove_friend_currency(remove_friend_currency_cmd, conn_pair, node_report)
                .await?
        }
        ConfigCmd::ResetFriend(reset_friend_cmd) => {
            config_reset_friend(reset_friend_cmd, conn_pair, node_report).await?
        }
    }

    Ok(())
}
