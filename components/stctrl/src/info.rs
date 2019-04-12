use std::io;
use std::path::PathBuf;

use prettytable::Table;
use structopt::StructOpt;

use app::report::{
    ChannelStatusReport, FriendReport, FriendStatusReport, NodeReport, RequestsStatusReport,
};
use app::ser_string::public_key_to_string;
use app::{store_friend_to_file, AppReport, FriendAddress, NodeConnection, RelayAddress};

use crate::file::token::store_token_to_file;
use crate::utils::friend_public_key_by_name;

/// Display local public key (Used as address for sending funds)
#[derive(Clone, Debug, StructOpt)]
pub struct PublicKeyCmd {}

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
    #[structopt(short = "n", name = "name")]
    pub friend_name: String,
    /// Path for output token file
    #[structopt(short = "o", name = "output")]
    pub output_file: PathBuf,
}

/// Display balance summary
#[derive(Clone, Debug, StructOpt)]
pub struct BalanceCmd {}

/// Export a ticket of this node's contact information
#[derive(Clone, Debug, StructOpt)]
pub struct ExportTicketCmd {
    #[structopt(short = "o", name = "output")]
    pub output_file: PathBuf,
}

#[derive(Clone, Debug, StructOpt)]
pub enum InfoCmd {
    /// Show local public key (Used as address for sending funds)
    #[structopt(name = "public-key")]
    PublicKey(PublicKeyCmd),
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
    /// Show current balance
    #[structopt(name = "balance")]
    Balance(BalanceCmd),
    /// Export ticket for this node
    #[structopt(name = "export-ticket")]
    ExportTicket(ExportTicketCmd),
}

#[derive(Debug)]
pub enum InfoError {
    GetReportError,
    BalanceOverflow,
    OutputFileAlreadyExists,
    StoreNodeToFileError,
    FriendNameNotFound,
    MissingLastIncomingMoveToken,
    StoreLastIncomingMoveTokenError,
    WriteError,
}

/// Get a most recently known node report:
async fn get_report(app_report: &mut AppReport) -> Result<NodeReport, InfoError> {
    let (node_report, incoming_mutations) =
        await!(app_report.incoming_reports()).map_err(|_| InfoError::GetReportError)?;
    // We currently don't need live updates about report mutations:
    drop(incoming_mutations);

    Ok(node_report)
}

/// Show local public key
pub async fn info_public_key(
    mut app_report: AppReport,
    writer: &mut impl io::Write,
) -> Result<(), InfoError> {
    let report = await!(get_report(&mut app_report))?;

    let public_key_string = public_key_to_string(&report.funder_report.local_public_key);
    writeln!(writer, "{}", &public_key_string).map_err(|_| InfoError::WriteError)?;
    Ok(())
}

pub async fn info_relays(
    mut app_report: AppReport,
    writer: &mut impl io::Write,
) -> Result<(), InfoError> {
    let report = await!(get_report(&mut app_report))?;

    let mut table = Table::new();
    // Add title:
    table.add_row(row!["relay name", "public key", "address"]);

    for named_relay_address in &report.funder_report.relays {
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
    mut app_report: AppReport,
    writer: &mut impl io::Write,
) -> Result<(), InfoError> {
    let report = await!(get_report(&mut app_report))?;

    let mut table = Table::new();
    // Add title:
    table.add_row(row!["index server name", "public key", "address"]);

    let opt_connected_server = &report.index_client_report.opt_connected_server;
    for named_index_server_address in &report.index_client_report.index_servers {
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

/// A user friendly string explaining the current channel status
fn friend_channel_status(friend_report: &FriendReport) -> String {
    let mut res = String::new();
    match &friend_report.channel_status {
        ChannelStatusReport::Consistent(tc_report) => {
            res += "C: ";
            let local_requests_str = requests_status_str(&tc_report.requests_status.local);
            let remote_requests_str = requests_status_str(&tc_report.requests_status.remote);

            res += &format!("LR={}, RR={}\n", local_requests_str, remote_requests_str);

            let balance = &tc_report.balance;
            res += &format!(
                "B  ={}\nLMD={}\nRMD={}\nLPD={}\nRPD={}\n",
                balance.balance,
                balance.local_max_debt,
                balance.remote_max_debt,
                balance.local_pending_debt,
                balance.remote_pending_debt
            );
        }
        ChannelStatusReport::Inconsistent(channel_inconsistent_report) => {
            res += "I:\n";
            res += &format!(
                "LT={}\n",
                channel_inconsistent_report.local_reset_terms_balance
            );
            match &channel_inconsistent_report.opt_remote_reset_terms {
                Some(remote_reset_terms) => {
                    // TODO: Possibly negate this value? Maybe we should do it at the funder?
                    res += &format!("RT={}", remote_reset_terms.balance_for_reset);
                }
                None => {
                    res += "RT=?";
                }
            }
        }
    }
    res
}

pub async fn info_friends(
    mut app_report: AppReport,
    writer: &mut impl io::Write,
) -> Result<(), InfoError> {
    let report = await!(get_report(&mut app_report))?;

    let mut table = Table::new();
    // Add titlek:
    table.set_titles(row!["st", "name", "balance"]);

    for (_friend_public_key, friend_report) in &report.funder_report.friends {
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
    mut app_report: AppReport,
) -> Result<(), InfoError> {
    let FriendLastTokenCmd {
        friend_name,
        output_file,
    } = friend_last_token_cmd;

    if output_file.exists() {
        return Err(InfoError::OutputFileAlreadyExists);
    }

    // Get a recent report:
    let (node_report, incoming_mutations) =
        await!(app_report.incoming_reports()).map_err(|_| InfoError::GetReportError)?;
    // We don't want to listen on incoming mutations:
    drop(incoming_mutations);

    let friend_public_key = friend_public_key_by_name(&node_report, &friend_name)
        .ok_or(InfoError::FriendNameNotFound)?;

    let friend_report = node_report
        .funder_report
        .friends
        .get(&friend_public_key)
        .unwrap();
    let last_incoming_move_token = match &friend_report.opt_last_incoming_move_token {
        Some(last_incoming_move_token) => last_incoming_move_token,
        // If the remote side have never sent any message, we might not have a
        // "last incoming move token":
        None => return Err(InfoError::MissingLastIncomingMoveToken),
    };

    store_token_to_file(last_incoming_move_token, &output_file)
        .map_err(|_| InfoError::StoreLastIncomingMoveTokenError)
}

/// Get an approximate value for mutual balance with a friend.
/// In case of an inconsistency we take the local reset terms to represent the balance.
fn friend_balance(friend_report: &FriendReport) -> i128 {
    match &friend_report.channel_status {
        ChannelStatusReport::Consistent(tc_report) => tc_report.balance.balance,
        ChannelStatusReport::Inconsistent(channel_inconsistent_report) => {
            channel_inconsistent_report.local_reset_terms_balance
        }
    }
}

pub async fn info_balance(
    mut app_report: AppReport,
    writer: &mut impl io::Write,
) -> Result<(), InfoError> {
    let report = await!(get_report(&mut app_report))?;

    let mut total_balance: i128 = 0;
    for (_friend_public_key, friend_report) in &report.funder_report.friends {
        total_balance = total_balance
            .checked_add(friend_balance(&friend_report))
            .ok_or(InfoError::BalanceOverflow)?;
    }

    writeln!(writer, "{}", total_balance).map_err(|_| InfoError::WriteError)?;
    Ok(())
}

pub async fn info_export_ticket(
    export_ticket_cmd: ExportTicketCmd,
    mut app_report: AppReport,
) -> Result<(), InfoError> {
    let ExportTicketCmd { output_file } = export_ticket_cmd;

    if output_file.exists() {
        return Err(InfoError::OutputFileAlreadyExists);
    }

    let report = await!(get_report(&mut app_report))?;
    let relays: Vec<RelayAddress> = report
        .funder_report
        .relays
        .into_iter()
        .map(Into::into)
        .collect();

    let node_address = FriendAddress {
        public_key: report.funder_report.local_public_key.clone(),
        relays,
    };

    store_friend_to_file(&node_address, &output_file)
        .map_err(|_| InfoError::StoreNodeToFileError)?;

    Ok(())
}

pub async fn info(
    info_cmd: InfoCmd,
    mut node_connection: NodeConnection,
    writer: &mut impl io::Write,
) -> Result<(), InfoError> {
    let app_report = node_connection.report().clone();

    match info_cmd {
        InfoCmd::PublicKey(_public_key_cmd) => await!(info_public_key(app_report, writer))?,
        InfoCmd::Relays(_relays_cmd) => await!(info_relays(app_report, writer))?,
        InfoCmd::Index(_index_cmd) => await!(info_index(app_report, writer))?,
        InfoCmd::Friends(_friends_cmd) => await!(info_friends(app_report, writer))?,
        InfoCmd::FriendLastToken(friend_last_token_cmd) => {
            await!(info_friend_last_token(friend_last_token_cmd, app_report))?
        }
        InfoCmd::Balance(_balance_cmd) => await!(info_balance(app_report, writer))?,
        InfoCmd::ExportTicket(export_ticket_cmd) => {
            await!(info_export_ticket(export_ticket_cmd, app_report))?
        }
    }
    Ok(())
}
