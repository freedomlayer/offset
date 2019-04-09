use std::path::PathBuf;

use clap::ArgMatches;
use prettytable::Table;

use app::report::{ChannelStatusReport, FriendReport, FriendStatusReport, NodeReport};
use app::ser_string::public_key_to_string;
use app::{store_friend_to_file, AppReport, FriendAddress, NodeConnection, RelayAddress};

use crate::file::token::store_token_to_file;
use crate::utils::friend_public_key_by_name;

#[derive(Debug)]
pub enum InfoError {
    GetReportError,
    BalanceOverflow,
    OutputFileAlreadyExists,
    StoreNodeToFileError,
    FriendNameNotFound,
    MissingLastIncomingMoveToken,
    StoreLastIncomingMoveTokenError,
}

/// Get a most recently known node report:
async fn get_report(app_report: &mut AppReport) -> Result<NodeReport, InfoError> {
    let (node_report, incoming_mutations) =
        await!(app_report.incoming_reports()).map_err(|_| InfoError::GetReportError)?;
    // We currently don't need live updates about report mutations:
    drop(incoming_mutations);

    Ok(node_report)
}

pub async fn info_relays(mut app_report: AppReport) -> Result<(), InfoError> {
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
    table.printstd();
    Ok(())
}

pub async fn info_index(mut app_report: AppReport) -> Result<(), InfoError> {
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
    table.printstd();
    Ok(())
}

/// A user friendly string explaining the current channel status
fn friend_channel_status(friend_report: &FriendReport) -> String {
    let mut res = String::new();
    match &friend_report.channel_status {
        ChannelStatusReport::Consistent(tc_report) => {
            res += "[Consistent]\n";
            res += &format!("balance = {}\n", tc_report.balance.balance);
            res += &format!("local_requests = {:?}\n", tc_report.requests_status.local);
            res += &format!("remote_requests = {:?}", tc_report.requests_status.remote);
        }
        ChannelStatusReport::Inconsistent(channel_inconsistent_report) => {
            res += "[Inconsistent]\n";
            res += &format!(
                "local_terms = {}",
                channel_inconsistent_report.local_reset_terms_balance
            );
            match &channel_inconsistent_report.opt_remote_reset_terms {
                Some(remote_reset_terms) => {
                    // TODO: Possibly negate this value? Maybe we should do it at the funder?
                    res += &format!("remote_terms = {}", remote_reset_terms.balance_for_reset);
                }
                None => {
                    res += "remote terms unknown";
                }
            }
        }
    }
    res
}

pub async fn info_friends(mut app_report: AppReport) -> Result<(), InfoError> {
    let report = await!(get_report(&mut app_report))?;

    let mut table = Table::new();
    // Add title:
    table.add_row(row!["friend name", "status", "liveness", "channel status"]);

    for (_friend_public_key, friend_report) in &report.funder_report.friends {
        // Is the friend enabled?
        let status_str = if friend_report.status == FriendStatusReport::Enabled {
            "enabled"
        } else {
            "disabled"
        };

        let liveness_str = if friend_report.liveness.is_online() {
            "online"
        } else {
            "offline"
        };

        table.add_row(row![
            friend_report.name,
            status_str,
            liveness_str,
            friend_channel_status(&friend_report)
        ]);
    }

    table.printstd();
    Ok(())
}

/// Obtain the last incoming move token messages from a friend.  
/// This is the last signed commitment made by the friend to the mutual balance.
pub async fn info_last_friend_token<'a>(
    matches: &'a ArgMatches<'a>,
    mut app_report: AppReport,
) -> Result<(), InfoError> {
    let friend_name = matches.value_of("friend_name").unwrap();
    let output_file = matches.value_of("output_file").unwrap();
    let output_pathbuf = PathBuf::from(output_file);

    if output_pathbuf.exists() {
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

    store_token_to_file(last_incoming_move_token, &output_pathbuf)
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

pub async fn info_balance(mut app_report: AppReport) -> Result<(), InfoError> {
    let report = await!(get_report(&mut app_report))?;

    let total_balance: i128 = 0;
    for (_friend_public_key, friend_report) in &report.funder_report.friends {
        total_balance
            .checked_add(friend_balance(&friend_report))
            .ok_or(InfoError::BalanceOverflow)?;
    }

    println!("Total balance: {}", total_balance);
    Ok(())
}

pub async fn info_export_ticket<'a>(
    matches: &'a ArgMatches<'a>,
    mut app_report: AppReport,
) -> Result<(), InfoError> {
    let output_file = matches.value_of("output_file").unwrap();
    let output_pathbuf = PathBuf::from(output_file);

    if output_pathbuf.exists() {
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

    store_friend_to_file(&node_address, &output_pathbuf)
        .map_err(|_| InfoError::StoreNodeToFileError)?;

    Ok(())
}

pub async fn info<'a>(
    matches: &'a ArgMatches<'a>,
    mut node_connection: NodeConnection,
) -> Result<(), InfoError> {
    let app_report = node_connection.report().clone();

    match matches.subcommand() {
        ("relays", Some(_matches)) => await!(info_relays(app_report))?,
        ("index", Some(_matches)) => await!(info_index(app_report))?,
        ("friends", Some(_matches)) => await!(info_friends(app_report))?,
        ("last-friend-token", Some(matches)) => {
            await!(info_last_friend_token(matches, app_report))?
        }
        ("balance", Some(_matches)) => await!(info_balance(app_report))?,
        ("export-ticket", Some(matches)) => await!(info_export_ticket(matches, app_report))?,
        _ => unreachable!(),
    }

    Ok(())
}
