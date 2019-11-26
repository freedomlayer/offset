use std::fs::File;
use std::io::{self, Write};
use std::path::PathBuf;

use prettytable::Table;
use structopt::StructOpt;

use derive_more::From;

use app::common::RelayAddress;
use app::report::{
    ChannelStatusReport, CurrencyReport, FriendReport, FriendStatusReport, NodeReport,
    RequestsStatusReport,
};
use app::ser_string::public_key_to_string;

use app::conn::ConnPairApp;
use app::file::{FriendAddressFile, FriendFile, RelayAddressFile};
use app::ser_string::{serialize_to_string, StringSerdeError};

use crate::file::TokenFile;

use crate::utils::friend_public_key_by_name;

/*
/// Display local public key (Used as address for sending funds)
#[derive(Clone, Debug, StructOpt)]
pub struct PublicKeyCmd {}
*/

/// Show all configured relays
#[derive(Clone, Debug, StructOpt)]
pub struct RelaysCmd {}

/// Show all configured index servers
#[derive(Clone, Debug, StructOpt)]
pub struct IndexCmd {}

/// Show all configured friend servers
#[derive(Clone, Debug, StructOpt)]
pub struct FriendsCmd {}

/// Export last obtained token from a friend
#[derive(Clone, Debug, StructOpt)]
pub struct FriendLastTokenCmd {
    /// Friend's name
    #[structopt(short = "n", long = "name")]
    pub friend_name: String,
    /// Path for output token file
    #[structopt(short = "t", long = "token")]
    pub token_path: PathBuf,
}

/// Display balance summary
#[derive(Clone, Debug, StructOpt)]
pub struct BalanceCmd {}

/// Export a ticket of this node's contact information
#[derive(Clone, Debug, StructOpt)]
pub struct ExportTicketCmd {
    /// Path to output ticket file
    #[structopt(short = "t", long = "ticket")]
    pub ticket_path: PathBuf,
}

#[derive(Clone, Debug, StructOpt)]
pub enum InfoCmd {
    // /// Show local public key (Used as address for sending funds)
    // #[structopt(name = "public-key")]
    // PublicKey(PublicKeyCmd),
    /// Show information about configured relay servers
    #[structopt(name = "relays")]
    Relays(RelaysCmd),
    /// Show information about configured index servers
    #[structopt(name = "index")]
    Index(IndexCmd),
    /// Show information about configured friends
    #[structopt(name = "friends")]
    Friends(FriendsCmd),
    /// Export friend's last token
    #[structopt(name = "friend-last-token")]
    FriendLastToken(FriendLastTokenCmd),
    // /// Show current balance
    // #[structopt(name = "balance")]
    // Balance(BalanceCmd),
    /// Export ticket for this node
    #[structopt(name = "export-ticket")]
    ExportTicket(ExportTicketCmd),
}

#[derive(Debug, From)]
pub enum InfoError {
    GetReportError,
    BalanceOverflow,
    OutputFileAlreadyExists,
    StoreNodeToFileError,
    FriendNameNotFound,
    MissingLastIncomingMoveToken,
    StoreLastIncomingMoveTokenError,
    WriteError,
    TokenInvalid,
    LoadTokenError,
    LoadInvoiceError,
    LoadReceiptError,
    InvalidReceipt,
    DestPaymentMismatch,
    InvoiceIdMismatch,
    IoError(std::io::Error),
    StringSerdeError(StringSerdeError),
}

/*
/// Get a most recently known node report:
async fn get_report(app_report: &mut AppReport) -> Result<NodeReport, InfoError> {
    let (node_report, incoming_mutations) = app_report
        .incoming_reports()
        .await
        .map_err(|_| InfoError::GetReportError)?;
    // We currently don't need live updates about report mutations:
    drop(incoming_mutations);

    Ok(node_report)
}
*/

/*
/// Show local public key
pub async fn info_public_key(
    mut app_report: AppReport,
    writer: &mut impl io::Write,
) -> Result<(), InfoError> {
    let report = get_report(&mut app_report).await?;

    let public_key_string = public_key_to_string(&report.funder_report.local_public_key);
    writeln!(writer, "{}", &public_key_string).map_err(|_| InfoError::WriteError)?;
    Ok(())
}
*/

pub async fn info_relays(
    node_report: &NodeReport,
    writer: &mut impl io::Write,
) -> Result<(), InfoError> {
    let mut table = Table::new();
    // Add title:
    table.set_titles(row!["relay name", "public key", "address"]);

    for named_relay_address in &node_report.funder_report.relays {
        let pk_string = public_key_to_string(&named_relay_address.public_key);
        table.add_row(row![
            named_relay_address.name,
            pk_string,
            named_relay_address.address
        ]);
    }
    if !table.is_empty() {
        table.print(writer).map_err(|_| InfoError::WriteError)?;
    } else {
        writeln!(writer, "No configured relay servers.").map_err(|_| InfoError::WriteError)?;
    }
    Ok(())
}

pub async fn info_index(
    node_report: &NodeReport,
    writer: &mut impl io::Write,
) -> Result<(), InfoError> {
    let mut table = Table::new();
    // Add title:
    table.set_titles(row!["index server name", "public key", "address"]);

    let opt_connected_server = &node_report.index_client_report.opt_connected_server;
    for named_index_server_address in &node_report.index_client_report.index_servers {
        // The currently used index will have (*) next to his name:
        let name = if opt_connected_server.as_ref() == Some(&named_index_server_address.public_key)
        {
            named_index_server_address.name.clone() + " (*)"
        } else {
            named_index_server_address.name.clone()
        };

        let pk_string = public_key_to_string(&named_index_server_address.public_key);
        table.add_row(row![name, pk_string, named_index_server_address.address]);
    }
    if !table.is_empty() {
        table.print(writer).map_err(|_| InfoError::WriteError)?;
    } else {
        writeln!(writer, "No configured index servers.").map_err(|_| InfoError::WriteError)?;
    }
    Ok(())
}

/// Return a string that represents requests status.
/// "+" means open, "-" means closed
fn requests_status_str(requests_status_report: &RequestsStatusReport) -> String {
    if requests_status_report == &RequestsStatusReport::Open {
        "+"
    } else {
        "-"
    }
    .to_owned()
}

/*
/// Check if a friend state is consistent
fn is_consistent(friend_report: &FriendReport) -> bool {
    match &friend_report.channel_status {
        ChannelStatusReport::Consistent(_) => true,
        ChannelStatusReport::Inconsistent(_) => false,
    }
}
*/

fn currency_report_str(currency_report: &CurrencyReport) -> String {
    let mut res = String::new();

    let local_requests_str = requests_status_str(&currency_report.requests_status.local);
    let remote_requests_str = requests_status_str(&currency_report.requests_status.remote);

    res += &format!("LR={}, RR={}\n", local_requests_str, remote_requests_str);

    let balance = &currency_report.balance;
    res += &format!(
        "B  ={}\nLMD={}\nRMD={}\nLPD={}\nRPD={}\n",
        balance.balance,
        balance.local_max_debt,
        balance.remote_max_debt,
        balance.local_pending_debt,
        balance.remote_pending_debt
    );

    res
}

/// A user friendly string explaining the current channel status
fn friend_channel_status(friend_report: &FriendReport) -> String {
    let mut res = String::new();
    match &friend_report.channel_status {
        ChannelStatusReport::Consistent(channel_consistent_report) => {
            res += "Consistent:\n";

            for currency_report in &channel_consistent_report.currency_reports {
                res += &format!(
                    "- {}: {}\n",
                    currency_report.currency,
                    currency_report_str(&currency_report)
                );
            }
        }
        ChannelStatusReport::Inconsistent(channel_inconsistent_report) => {
            res += "Inconsistent:\n";
            res += "Local Reset Terms:\n";
            for currency_balance in &channel_inconsistent_report.local_reset_terms {
                res += &format!(
                    "- {}: {}\n",
                    currency_balance.currency, currency_balance.balance
                );
            }
            match &channel_inconsistent_report.opt_remote_reset_terms {
                Some(remote_reset_terms) => {
                    res += "Remote Reset Terms:\n";
                    for currency_balance in &remote_reset_terms.balance_for_reset {
                        res += &format!(
                            "- {}: {}\n",
                            currency_balance.currency, currency_balance.balance
                        );
                    }
                }
                None => {
                    res += "Remote Reset Terms: Unknown\n";
                }
            }
        }
    }
    res
}

pub async fn info_friends(
    node_report: &NodeReport,
    writer: &mut impl io::Write,
) -> Result<(), InfoError> {
    let mut table = Table::new();
    // Add titlek:
    table.set_titles(row!["st", "name", "balance"]);

    for friend_report in node_report.funder_report.friends.values() {
        // Is the friend enabled?
        let status_str = if friend_report.status == FriendStatusReport::Enabled {
            "E"
        } else {
            "D"
        };

        let liveness_str = if friend_report.liveness.is_online() {
            "+"
        } else {
            "-"
        };

        let mut status_string = String::new();
        status_string += status_str;
        status_string += liveness_str;

        table.add_row(row![
            status_string,
            friend_report.name,
            friend_channel_status(&friend_report),
        ]);
    }

    if !table.is_empty() {
        table.print(writer).map_err(|_| InfoError::WriteError)?;
    } else {
        writeln!(writer, "No configured friends.").map_err(|_| InfoError::WriteError)?;
    }
    Ok(())
}

/// Obtain the last incoming move token messages from a friend.  
/// This is the last signed commitment made by the friend to the mutual balance.
pub async fn info_friend_last_token(
    friend_last_token_cmd: FriendLastTokenCmd,
    node_report: &NodeReport,
) -> Result<(), InfoError> {
    let FriendLastTokenCmd {
        friend_name,
        token_path,
    } = friend_last_token_cmd;

    if token_path.exists() {
        return Err(InfoError::OutputFileAlreadyExists);
    }

    let friend_public_key = friend_public_key_by_name(node_report, &friend_name)
        .ok_or(InfoError::FriendNameNotFound)?;

    let friend_report = node_report
        .funder_report
        .friends
        .get(&friend_public_key)
        .unwrap();
    let token_file: TokenFile = match &friend_report.opt_last_incoming_move_token {
        Some(last_incoming_move_token) => last_incoming_move_token,
        // If the remote side have never sent any message, we might not have a
        // "last incoming move token":
        None => return Err(InfoError::MissingLastIncomingMoveToken),
    }
    .clone()
    .into();

    let mut file = File::create(token_path)?;
    file.write_all(&serialize_to_string(&token_file)?.as_bytes())?;
    Ok(())
}

/*
/// Get an approximate value for mutual balance with a friend.
/// In case of an inconsistency we take the local reset terms to represent the balance.
fn friend_balances(friend_report: &FriendReport) -> i128 {
    match &friend_report.channel_status {
        ChannelStatusReport::Consistent(channel_consistent_report) => {
            channel_consistent_report.tc_report.balance.balance
        }
        ChannelStatusReport::Inconsistent(channel_inconsistent_report) => {
            channel_inconsistent_report.local_reset_terms
        }
    }
}

pub async fn info_balance(
    mut app_report: AppReport,
    writer: &mut impl io::Write,
) -> Result<(), InfoError> {
    let report = get_report(&mut app_report).await?;

    let mut total_balance: i128 = 0;
    for friend_report in report.funder_report.friends.values() {
        total_balance = total_balance
            .checked_add(friend_balance(&friend_report))
            .ok_or(InfoError::BalanceOverflow)?;
    }

    writeln!(writer, "{}", total_balance).map_err(|_| InfoError::WriteError)?;
    Ok(())
}
*/

pub async fn info_export_ticket(
    export_ticket_cmd: ExportTicketCmd,
    node_report: &NodeReport,
) -> Result<(), InfoError> {
    let ExportTicketCmd { ticket_path } = export_ticket_cmd;

    if ticket_path.exists() {
        return Err(InfoError::OutputFileAlreadyExists);
    }

    let relays: Vec<RelayAddress> = node_report
        .funder_report
        .relays
        .clone()
        .into_iter()
        .map(Into::into)
        .collect();

    let friend_address_file = FriendAddressFile {
        public_key: node_report.funder_report.local_public_key.clone(),
        relays: relays.into_iter().map(RelayAddressFile::from).collect(),
    };

    let mut file = File::create(ticket_path)?;
    file.write_all(&serialize_to_string(&friend_address_file)?.as_bytes())?;

    Ok(())
}

pub async fn info(
    info_cmd: InfoCmd,
    node_report: &NodeReport,
    writer: &mut impl io::Write,
) -> Result<(), InfoError> {
    match info_cmd {
        // InfoCmd::PublicKey(_public_key_cmd) => info_public_key(node_report, writer).await?,
        InfoCmd::Relays(_relays_cmd) => info_relays(node_report, writer).await?,
        InfoCmd::Index(_index_cmd) => info_index(node_report, writer).await?,
        InfoCmd::Friends(_friends_cmd) => info_friends(node_report, writer).await?,
        InfoCmd::FriendLastToken(friend_last_token_cmd) => {
            info_friend_last_token(friend_last_token_cmd, node_report).await?
        }
        // InfoCmd::Balance(_balance_cmd) => info_balance(node_report, writer).await?,
        InfoCmd::ExportTicket(export_ticket_cmd) => {
            info_export_ticket(export_ticket_cmd, node_report).await?
        }
    }
    Ok(())
}
