use std::io;
use std::path::PathBuf;

use futures::future::select_all;

use app::ser_string::string_to_public_key;
use app::{AppRoutes, AppSendFunds, NodeConnection, PublicKey, MultiCommit};

use structopt::StructOpt;

use app::gen::{gen_uid, gen_payment_id};
use app::invoice::{InvoiceId, INVOICE_ID_LEN};
use app::route::{FriendsRoute, MultiRoute};

use crate::file::invoice::load_invoice_from_file;
use crate::multi_route_util::choose_multi_route;
use crate::file::multi_commit::store_multi_commit_to_file;
// use crate::file::receipt::store_receipt_to_file;

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
    /// Output commit file
    #[structopt(parse(from_os_str), short = "c", long = "commit")]
    pub commit_file: PathBuf,
}

/// Funds sending related commands
#[derive(Clone, Debug, StructOpt)]
pub enum FundsCmd {
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
    CommitFileAlreadyExists,
    ReceiptFileAlreadyExists,
    StoreReceiptError,
    ReceiptAckError,
    LoadInvoiceError,
    WriteError,
    CreatePaymentFailed,
    CreateTransactionFailed,
    StoreCommitError,
}



/*
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
    let amounts = safe_multi_route_amounts(&multi_route).unwrap();

    // TODO: 
    // - Create Payment
    // - Create a transaction for every route
    // - Wait until the first a first failure happens or all transactions succeeded.
    // - Close payment
    // - Return result (MultiCommit or failure)

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
*/

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
        commit_file,
    } = pay_invoice_cmd;


    // Make sure that we will be able to write the MultiCommit
    // before we do the actual payment:
    if commit_file.exists() {
        return Err(FundsError::CommitFileAlreadyExists);
    }

    let invoice =
        load_invoice_from_file(&invoice_file).map_err(|_| FundsError::LoadInvoiceError)?;

    // TODO: We might get routes with the exact capacity,
    // but this will not be enough for sending our amount because
    // we also need to pay nodes on the way.
    // We might need to solve this issue at the index server side
    // (Should the Server take into account the extra credits that should be paid along the way?).
    let multi_routes = await!(app_routes.request_routes(
        invoice.dest_payment,
        local_public_key, // source
        invoice.dest_public_key.clone(),
        None
    )) // No exclusion of edges
    .map_err(|_| FundsError::AppRoutesError)?;


    let (route_index, multi_route_choice) = choose_multi_route(&multi_routes, invoice.dest_payment)
        .ok_or(FundsError::NoSuitableRoute)?;
    let multi_route = &multi_routes[route_index];


    // Calculate total fees:
    // TODO: Possibly ask the user if he wants to pay this amount of fees at this point.
    let mut total_fees = 0u128;
    for (route_index, dest_payment) in &multi_route_choice {
        let fee = multi_route.routes[*route_index].rate.calc_fee(*dest_payment).unwrap();
        total_fees = total_fees.checked_add(fee).unwrap();
    }
    writeln!(writer, "Total fees: {}", total_fees).map_err(|_| FundsError::WriteError)?;

    // Create a new payment
    let payment_id = gen_payment_id();
    await!(app_send_funds.create_payment(payment_id, 
                                  invoice.invoice_id.clone(),
                                  invoice.dest_payment.clone(),
                                  invoice.dest_public_key.clone()))
        .map_err(|_| FundsError::CreatePaymentFailed)?;

    // TODO: Run all of those `create_transaction`-s at the same time:
    // TODO:
    // - Create new transactions (One for every route). On the first failure cancel all
    //      transactions. Succeed only if all transactions succeed.
    
    let mut fut_list = Vec::new();
    for (route_index, dest_payment) in &multi_route_choice {
        let route = &multi_route.routes[*route_index];
        let request_id = gen_uid();
        let mut c_app_send_funds = app_send_funds.clone();
        fut_list.push(
            // TODO: Possibly a more efficient way than Box::pin?
            Box::pin(async move {
                await!(c_app_send_funds.create_transaction(
                    payment_id.clone(),
                    request_id,
                    route.route.clone(),
                    *dest_payment,
                    route.rate.calc_fee(*dest_payment).unwrap(),
                ))
            })
        );
    }

    let mut commits = Vec::new();
    for _ in 0 .. multi_route_choice.len() {
        let (output, fut_index, new_fut_list) = await!(select_all(fut_list));
        match output {
            Ok(commit) => commits.push(commit),
            Err(_) => return Err(FundsError::CreateTransactionFailed),
        }
        fut_list = new_fut_list;
    }

    let multi_commit = MultiCommit {
        invoice_id: invoice.invoice_id.clone(),
        total_dest_payment: invoice.dest_payment,
        commits,
    };

    writeln!(writer, "Payment successful!").map_err(|_| FundsError::WriteError)?;

    // Store MultiCommit to file:
    store_multi_commit_to_file(&multi_commit, &commit_file)
        .map_err(|_| FundsError::StoreCommitError)?;

    // TODO:
    // - Print back PaymentId, to allow the user get the receipt later.

    unimplemented!();

}

// TODO: Add a command to check information about payment progress.

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
