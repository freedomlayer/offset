use std::path::PathBuf;

use app::ser_string::string_to_public_key;
use app::{AppRoutes, AppSendFunds, NodeConnection, PublicKey};
use clap::ArgMatches;

use app::gen::gen_uid;
use app::invoice::{InvoiceId, INVOICE_ID_LEN};
use app::route::{FriendsRoute, RouteWithCapacity};

use crate::file::receipt::store_receipt_to_file;

#[derive(Debug)]
pub enum FundsError {
    GetReportError,
    NoFundsPermissions,
    NoRoutesPermissions,
    InvalidDestination,
    ParseAmountError,
    AppRoutesError,
    SendFundsError,
    NoSuitableRoute,
    ReceiptFileAlreadyExists,
    StoreReceiptError,
    ReceiptAckError,
}

/// Choose a route for pushing `amount` credits
fn choose_route(
    routes_with_capacity: Vec<RouteWithCapacity>,
    amount: u128,
) -> Result<FriendsRoute, FundsError> {
    // We naively select the first route we find suitable:
    // TODO: Possibly improve this later:
    for route_with_capacity in routes_with_capacity {
        // TODO: Is this dangerous? How can we do this safely?
        let length = route_with_capacity.route.len() as u128;

        // For route of length 2 we pay 0. (source and destination are included)
        // For route of length 3 we pay 1.
        // ...
        let extra: u128 = if let Some(extra) = length.checked_sub(2) {
            extra
        } else {
            // This is an invalid route
            warn!(
                "Received invalid route of length: {}. Skipping route",
                route_with_capacity.route.len()
            );
            continue;
        };

        let total: u128 = if let Some(total) = extra.checked_add(amount) {
            total
        } else {
            warn!("Overflow when calculating total payment. Skipping route");
            continue;
        };

        if total <= route_with_capacity.capacity {
            return Ok(route_with_capacity.route);
        }
    }
    Err(FundsError::NoSuitableRoute)
}

async fn funds_send_raw<'a>(
    matches: &'a ArgMatches<'a>,
    local_public_key: PublicKey,
    mut app_routes: AppRoutes,
    mut app_send_funds: AppSendFunds,
) -> Result<(), FundsError> {
    let destination_str = matches.value_of("destination").unwrap();
    let amount_str = matches.value_of("amount").unwrap();

    let opt_receipt_file = matches.value_of("receipt");
    let opt_receipt_pathbuf = opt_receipt_file.map(PathBuf::from);

    // In case the user wants a receipt, make sure that we will be able to write the receipt
    // before we do the actual payment:
    if let Some(receipt_pathbuf) = &opt_receipt_pathbuf {
        if receipt_pathbuf.exists() {
            return Err(FundsError::ReceiptFileAlreadyExists);
        }
    }

    let amount = amount_str
        .parse::<u128>()
        .map_err(|_| FundsError::ParseAmountError)?;

    // Destination public key:
    let destination =
        string_to_public_key(&destination_str).map_err(|_| FundsError::InvalidDestination)?;

    // TODO: We might get routes with the exact capacity,
    // but this will not be enough for sending our amount because
    // we also need to pay nodes on the way.
    // We might need to solve this issue at the index server side
    // (Should the Server take into account the extra credits that should be paid along the way?).
    let routes_with_capacity = await!(app_routes.request_routes(
        amount,
        local_public_key, // source
        destination,
        None
    )) // No exclusion of edges
    .map_err(|_| FundsError::AppRoutesError)?;

    let route = choose_route(routes_with_capacity, amount)?;
    let fees = route.len().checked_sub(2).unwrap();

    // A trivial invoice:
    let request_id = gen_uid();
    let invoice_id = InvoiceId::from(&[0; INVOICE_ID_LEN]);

    let receipt = await!(app_send_funds.request_send_funds(request_id, route, invoice_id, amount))
        .map_err(|_| FundsError::SendFundsError)?;

    println!("Payment successful!");
    println!("Fees: {}", fees);

    // If the user wanted a receipt, we provide one:
    if let Some(receipt_pathbuf) = opt_receipt_pathbuf {
        // Store receipt to file:
        store_receipt_to_file(&receipt, &receipt_pathbuf).map_err(|_| FundsError::StoreReceiptError)?;
    }

    // We only send the ack if we managed to get the receipt:
    await!(app_send_funds.receipt_ack(request_id, receipt)).map_err(|_| FundsError::ReceiptAckError)
}

pub async fn funds<'a>(
    matches: &'a ArgMatches<'a>,
    mut node_connection: NodeConnection,
) -> Result<(), FundsError> {
    // Get our local public key:
    let mut app_report = node_connection.report().clone();
    let (node_report, incoming_mutations) =
        await!(app_report.incoming_reports()).map_err(|_| FundsError::GetReportError)?;
    // We currently don't need live updates about report mutations:
    drop(incoming_mutations);

    let local_public_key = node_report.funder_report.local_public_key.clone();

    let app_send_funds = node_connection
        .send_funds()
        .ok_or(FundsError::NoFundsPermissions)?
        .clone();

    let app_routes = node_connection
        .routes()
        .ok_or(FundsError::NoRoutesPermissions)?
        .clone();

    match matches.subcommand() {
        ("send", Some(matches)) => await!(funds_send_raw(
            matches,
            local_public_key,
            app_routes,
            app_send_funds
        ))?,
        _ => unreachable!(),
    }

    Ok(())
}
