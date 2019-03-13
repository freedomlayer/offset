use clap::ArgMatches;

use app::{NodeConnection, AppReport, public_key_to_string};
use app::report::NodeReport;

pub enum InfoError {
    GetReportError,
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
    for named_relay_address in report.funder_report.relays {
        let pk_string = public_key_to_string(&named_relay_address.public_key);
        println!("{}: {} {}", named_relay_address.name, 
                              pk_string,
                              named_relay_address.address);

    }

    unimplemented!();
}

pub async fn info_index(_app_report: AppReport) -> Result<(), InfoError> {
    unimplemented!();
}

pub async fn info_friends(_app_report: AppReport) -> Result<(), InfoError> {
    unimplemented!();
}

pub async fn info_last_friend_token<'a>(_matches: &'a ArgMatches<'a>, 
                                    _app_report: AppReport) -> Result<(), InfoError> {
    unimplemented!();
}

pub async fn info_balance<'a>(_app_report: AppReport) -> Result<(), InfoError> {
    unimplemented!();
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
