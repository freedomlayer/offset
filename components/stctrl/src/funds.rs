use std::io;
use std::path::PathBuf;

use app::ser_string::string_to_public_key;
use app::{AppRoutes, AppSendFunds, NodeConnection, PublicKey};

use structopt::StructOpt;

use app::gen::gen_uid;
use app::invoice::{InvoiceId, INVOICE_ID_LEN};
use app::route::{FriendsRoute, MultiRoute};

use crate::file::invoice::load_invoice_from_file;
use crate::file::receipt::store_receipt_to_file;

/// Send funds to a remote destination
#[derive(Clone, Debug, StructOpt)]
pub struct SendFundsCmd {
    /// recipient's public key
    #[structopt(short = "d", long = "dest")]
    pub destination_str: String,
    /// Amount of credits to send
    #[structopt(short = "a", long = "amount")]
    pub dest_payment: u128,
    /// Output receipt file
    #[structopt(parse(from_os_str), short = "r", long = "receipt")]
    pub opt_receipt_file: Option<PathBuf>,
}

/// Pay an invoice
#[derive(Clone, Debug, StructOpt)]
pub struct PayInvoiceCmd {
    /// Path to invoice file to pay
    #[structopt(parse(from_os_str), short = "i", long = "invoice")]
    pub invoice_file: PathBuf,
    /// Output receipt file
    #[structopt(parse(from_os_str), short = "r", long = "receipt")]
    pub receipt_file: PathBuf,
}

/// Funds sending related commands
#[derive(Clone, Debug, StructOpt)]
pub enum FundsCmd {
    /// Send funds to a remote destination (Using destination public key)
    #[structopt(name = "send-funds")]
    SendFunds(SendFundsCmd),
    /// Pay an invoice (Using an invoice file)
    #[structopt(name = "pay-invoice")]
    PayInvoice(PayInvoiceCmd),
}

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
    LoadInvoiceError,
    WriteError,
}

// TODO: The algorithm here might not be accurate in all cases.
// Fix later.
/// Can we push the given amount of credits through this multi route?
fn is_good_multi_route(
    multi_route: &MultiRoute,
    mut amount: u128) -> bool {

    let mut credit_count = 0u128;
    let mut fees_count = 0u128;

    for route_capacity_rate in multi_route.routes {
        let route_fees = route_capacity_rate.rate.calc_fees(route_capacity_rate.capacity)?;
        fees_count = fees_count.checked_add(route_fees)?;
        // Amount of capacity we are left with for actual payment:
        let left_capacity = route_capacity_rate.capacity.checked_sub(route_fees)?;
        credit_count = credit_count.checked_add(left_capacity)?;
    }

    credit_count >= amount

}

/// Choose a route for pushing `amount` credits
fn choose_multi_route(
    multi_routes: Vec<MultiRoute>,
    amount: u128,
) -> Result<MultiRoute, FundsError> {
    // We naively select the first multi-route we find suitable:
    // TODO: Possibly improve this later:
    for multi_route in multi_routes {
        if is_good_multi_route(&multi_route, amount) {
            return Ok(multi_route)
        }
    }
    Err(FundsError::NoSuitableRoute)
}

/// Send funds to a remote destination without using an invoice.
async fn funds_send_funds(
    send_raw_cmd: SendFundsCmd,
    local_public_key: PublicKey,
    mut app_routes: AppRoutes,
    mut app_send_funds: AppSendFunds,
    writer: &mut impl io::Write,
) -> Result<(), FundsError> {
    let SendFundsCmd {
        destination_str,
        dest_payment,
        opt_receipt_file,
    } = send_raw_cmd;

    // In case the user wants a receipt, make sure that we will be able to write the receipt
    // before we do the actual payment:
    if let Some(receipt_file) = &opt_receipt_file {
        if receipt_file.exists() {
            return Err(FundsError::ReceiptFileAlreadyExists);
        }
    }

    // Destination public key:
    let destination =
        string_to_public_key(&destination_str).map_err(|_| FundsError::InvalidDestination)?;

    // TODO: We might get routes with the exact capacity,
    // but this will not be enough for sending our amount because
    // we also need to pay nodes on the way.
    // We might need to solve this issue at the index server side
    // (Should the Server take into account the extra credits that should be paid along the way?).
    let routes_with_capacity = await!(app_routes.request_routes(
        dest_payment,
        local_public_key, // source
        destination,
        None
    )) // No exclusion of edges
    .map_err(|_| FundsError::AppRoutesError)?;

    let multi_route = choose_multi_route(routes_with_capacity, dest_payment)?;
    // TODO: Calculate fees?

    // A trivial invoice:
    let request_id = gen_uid();
    let invoice_id = InvoiceId::from(&[0; INVOICE_ID_LEN]);

    let receipt =
        await!(app_send_funds.request_send_funds(request_id, route, invoice_id, dest_payment))
            .map_err(|_| FundsError::SendFundsError)?;

    writeln!(writer, "Payment successful!").map_err(|_| FundsError::WriteError)?;
    writeln!(writer, "Fees: {}", fees).map_err(|_| FundsError::WriteError)?;

    // If the user wanted a receipt, we provide one:
    if let Some(receipt_file) = opt_receipt_file {
        // Store receipt to file:
        store_receipt_to_file(&receipt, &receipt_file)
            .map_err(|_| FundsError::StoreReceiptError)?;
    }

    // We only send the ack if we managed to get the receipt:
    await!(app_send_funds.receipt_ack(request_id, receipt)).map_err(|_| FundsError::ReceiptAckError)
}

/// Pay an invoice
async fn funds_pay_invoice(
    pay_invoice_cmd: PayInvoiceCmd,
    local_public_key: PublicKey,
    mut app_routes: AppRoutes,
    mut app_send_funds: AppSendFunds,
    writer: &mut impl io::Write,
) -> Result<(), FundsError> {
    let PayInvoiceCmd {
        invoice_file,
        receipt_file,
    } = pay_invoice_cmd;

    // Make sure that we will be able to write the receipt
    // before we do the actual payment:
    if receipt_file.exists() {
        return Err(FundsError::ReceiptFileAlreadyExists);
    }

    let invoice =
        load_invoice_from_file(&invoice_file).map_err(|_| FundsError::LoadInvoiceError)?;

    // TODO: We might get routes with the exact capacity,
    // but this will not be enough for sending our amount because
    // we also need to pay nodes on the way.
    // We might need to solve this issue at the index server side
    // (Should the Server take into account the extra credits that should be paid along the way?).
    let routes_with_capacity = await!(app_routes.request_routes(
        invoice.dest_payment,
        local_public_key, // source
        invoice.dest_public_key,
        None
    )) // No exclusion of edges
    .map_err(|_| FundsError::AppRoutesError)?;

    let route = choose_route(routes_with_capacity, invoice.dest_payment)?;
    let fees = route.len().checked_sub(2).unwrap();

    // Randomly generate a request id:
    let request_id = gen_uid();

    let receipt = await!(app_send_funds.request_send_funds(
        request_id,
        route,
        invoice.invoice_id,
        invoice.dest_payment
    ))
    .map_err(|_| FundsError::SendFundsError)?;

    writeln!(writer, "Payment successful!").map_err(|_| FundsError::WriteError)?;
    writeln!(writer, "Fees: {}", fees).map_err(|_| FundsError::WriteError)?;

    // Store receipt to file:
    store_receipt_to_file(&receipt, &receipt_file).map_err(|_| FundsError::StoreReceiptError)?;

    // We only send the ack if we managed to get the receipt:
    await!(app_send_funds.receipt_ack(request_id, receipt)).map_err(|_| FundsError::ReceiptAckError)
}
pub async fn funds(
    funds_cmd: FundsCmd,
    mut node_connection: NodeConnection,
    writer: &mut impl io::Write,
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

    match funds_cmd {
        FundsCmd::SendFunds(send_raw_cmd) => await!(funds_send_funds(
            send_raw_cmd,
            local_public_key,
            app_routes,
            app_send_funds,
            writer,
        ))?,
        FundsCmd::PayInvoice(pay_invoice_cmd) => await!(funds_pay_invoice(
            pay_invoice_cmd,
            local_public_key,
            app_routes,
            app_send_funds,
            writer,
        ))?,
    }

    Ok(())
}
