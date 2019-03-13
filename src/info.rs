use clap::ArgMatches;
use prettytable::Table;

use app::{NodeConnection, AppReport, public_key_to_string};
use app::report::{NodeReport, 
    FriendReport, ChannelStatusReport,
    FriendStatusReport};


#[derive(Debug)]
pub enum InfoError {
    GetReportError,
    BalanceOverflow,
}

/// Get a most recently known node report:
async fn get_report(app_report: &mut AppReport) -> Result<NodeReport, InfoError> {

    let (node_report, incoming_mutations) = await!(app_report.incoming_reports())
        .map_err(|_| InfoError::GetReportError)?;
    // We currently don't need live updates about report mutations:
    drop(incoming_mutations);

    Ok(node_report)
}

pub async fn info_relays(mut app_report: AppReport) -> Result<(), InfoError> {

    let report = await!(get_report(&mut app_report))?;

    let mut table = Table::new();
    // Add title:
    table.add_row(row!["relay name",
                       "public key", 
                       "address"]);

    for named_relay_address in &report.funder_report.relays {
        let pk_string = public_key_to_string(&named_relay_address.public_key);
        table.add_row(row![named_relay_address.name, 
                           pk_string, 
                           named_relay_address.address]);
    }
    table.printstd();
    Ok(())
}

pub async fn info_index(mut app_report: AppReport) -> Result<(), InfoError> {
    let report = await!(get_report(&mut app_report))?;

    let mut table = Table::new();
    // Add title:
    table.add_row(row!["index server name",
                       "public key", 
                       "address"]);


    let opt_connected_server = &report.index_client_report.opt_connected_server;
    for named_index_server_address in &report.index_client_report.index_servers {

        // The currently used index will have (*) next to his name:
        let name = if opt_connected_server.as_ref() == Some(&named_index_server_address.public_key) {
            named_index_server_address.name.clone() + " (*)"
        } else {
            named_index_server_address.name.clone()
        };

        let pk_string = public_key_to_string(&named_index_server_address.public_key);
        table.add_row(row![name,
                           pk_string, 
                           named_index_server_address.address]);
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
            res += &format!("balance = {}", tc_report.balance.balance);
        },
        ChannelStatusReport::Inconsistent(channel_inconsistent_report) => {
            res += "[Inconsistent]\n";
            res += &format!("local_terms = {}", channel_inconsistent_report.local_reset_terms_balance);
            match &channel_inconsistent_report.opt_remote_reset_terms {
                Some(remote_reset_terms) => {
                    // TODO: Possibly negate this value? Maybe we should do it at the funder?
                    res += &format!("remote_terms = {}", remote_reset_terms.balance_for_reset);
                },
                None => {
                    res += "remote terms unknown";
                },
            }
        },
    }
    res
}

pub async fn info_friends(mut app_report: AppReport) -> Result<(), InfoError> {
    let report = await!(get_report(&mut app_report))?;

    let mut table = Table::new();
    // Add title:
    table.add_row(row!["friend name",
                       "status",
                       "liveness",
                       "channel status"]);

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
        
        table.add_row(row![friend_report.name,
                           status_str,
                           liveness_str,
                           friend_channel_status(&friend_report)]);
    }

    table.printstd();
    Ok(())
}

pub async fn info_last_friend_token<'a>(_matches: &'a ArgMatches<'a>, 
                                    _app_report: AppReport) -> Result<(), InfoError> {
    // TODO: Should possibly export a file
    unimplemented!();
}

/// Get an approximate value for mutual balance with a friend.
/// In case of an inconsistency we take the local reset terms to represent the balance.
fn friend_balance(friend_report: &FriendReport) -> i128 {
    match &friend_report.channel_status {
        ChannelStatusReport::Consistent(tc_report) =>
            tc_report.balance.balance,
        ChannelStatusReport::Inconsistent(channel_inconsistent_report) =>
            channel_inconsistent_report.local_reset_terms_balance
    }
}

pub async fn info_balance(mut app_report: AppReport) -> Result<(), InfoError> {
    let report = await!(get_report(&mut app_report))?;

    let total_balance: i128 = 0;
    for (_friend_public_key, friend_report) in &report.funder_report.friends {
        total_balance.checked_add(friend_balance(&friend_report))
            .ok_or(InfoError::BalanceOverflow)?;
    }

    println!("Total balance: {}", total_balance);
    Ok(())
}

pub async fn info<'a>(matches: &'a ArgMatches<'a>, 
                      mut node_connection: NodeConnection) -> Result<(), InfoError> {

    let app_report = node_connection.report().clone();

    /*
    let idfile = matches.value_of("idfile").unwrap();
    let output = matches.value_of("output").unwrap();
    */

    match matches.subcommand() {
        ("relays", Some(_matches)) => await!(info_relays(app_report))?,
        ("index", Some(_matches)) => await!(info_index(app_report))?,
        ("friends", Some(_matches)) => await!(info_friends(app_report))?,
        ("last-friend-token", Some(matches)) => await!(info_last_friend_token(matches, app_report))?,
        ("balance", Some(_matches)) => await!(info_balance(app_report))?,
        _ => unreachable!(),
    }

    Ok(())
}
