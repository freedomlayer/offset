use std::io;
use std::path::PathBuf;

use futures::future::select_all;

use app::ser_string::{payment_id_to_string, string_to_payment_id};
use app::{AppBuyer, AppRoutes, MultiCommit, NodeConnection, PaymentStatus, PublicKey};

use structopt::StructOpt;

use app::gen::{gen_payment_id, gen_uid};

use crate::file::invoice::load_invoice_from_file;
use crate::file::multi_commit::store_multi_commit_to_file;
use crate::file::receipt::store_receipt_to_file;
use crate::multi_route_util::choose_multi_route;

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

/// Check payment status (And obtain receipt if successful)
#[derive(Clone, Debug, StructOpt)]
pub struct PaymentStatusCmd {
    /// Payment identifier
    #[structopt(short = "p", long = "payment")]
    pub payment_id: String,
    /// Output receipt file (in case a receipt is ready)
    #[structopt(parse(from_os_str), short = "r", long = "receipt")]
    pub receipt_file: PathBuf,
}

/// Funds sending related commands
#[derive(Clone, Debug, StructOpt)]
pub enum BuyerCmd {
    /// Pay an invoice (Using an invoice file)
    #[structopt(name = "pay-invoice")]
    PayInvoice(PayInvoiceCmd),
    #[structopt(name = "payment-status")]
    PaymentStatus(PaymentStatusCmd),
}

#[derive(Debug)]
pub enum BuyerError {
    GetReportError,
    NoBuyerPermissions,
    NoRoutesPermissions,
    InvalidDestination,
    ParseAmountError,
    AppRoutesError,
    SendBuyerError,
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
    RequestClosePaymentError,
    AckClosePaymentError,
    ParsePaymentIdError,
}

/// Pay an invoice
async fn buyer_pay_invoice(
    pay_invoice_cmd: PayInvoiceCmd,
    local_public_key: PublicKey,
    mut app_routes: AppRoutes,
    mut app_buyer: AppBuyer,
    writer: &mut impl io::Write,
) -> Result<(), BuyerError> {
    let PayInvoiceCmd {
        invoice_file,
        commit_file,
    } = pay_invoice_cmd;

    // Make sure that we will be able to write the MultiCommit
    // before we do the actual payment:
    if commit_file.exists() {
        return Err(BuyerError::CommitFileAlreadyExists);
    }

    let invoice =
        load_invoice_from_file(&invoice_file).map_err(|_| BuyerError::LoadInvoiceError)?;

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
    .map_err(|_| BuyerError::AppRoutesError)?;

    let (route_index, multi_route_choice) = choose_multi_route(&multi_routes, invoice.dest_payment)
        .ok_or(BuyerError::NoSuitableRoute)?;
    let multi_route = &multi_routes[route_index];

    // Calculate total fees:
    // TODO: Possibly ask the user if he wants to pay this amount of fees at this point.
    let mut total_fees = 0u128;
    for (route_index, dest_payment) in &multi_route_choice {
        let fee = multi_route.routes[*route_index]
            .rate
            .calc_fee(*dest_payment)
            .unwrap();
        total_fees = total_fees.checked_add(fee).unwrap();
    }
    writeln!(writer, "Total fees: {}", total_fees).map_err(|_| BuyerError::WriteError)?;

    // Create a new payment
    let payment_id = gen_payment_id();

    writeln!(
        writer,
        "Payment id: {:?}",
        payment_id_to_string(&payment_id)
    )
    .map_err(|_| BuyerError::WriteError)?;

    await!(app_buyer.create_payment(
        payment_id,
        invoice.invoice_id.clone(),
        invoice.dest_payment,
        invoice.dest_public_key.clone()
    ))
    .map_err(|_| BuyerError::CreatePaymentFailed)?;

    // Create new transactions (One for every route). On the first failure cancel all
    // transactions. Succeed only if all transactions succeed.

    let mut fut_list = Vec::new();
    for (route_index, dest_payment) in &multi_route_choice {
        let route = &multi_route.routes[*route_index];
        let request_id = gen_uid();
        let mut c_app_buyer = app_buyer.clone();
        fut_list.push(
            // TODO: Possibly a more efficient way than Box::pin?
            Box::pin(async move {
                await!(c_app_buyer.create_transaction(
                    payment_id,
                    request_id,
                    route.route.clone(),
                    *dest_payment,
                    route.rate.calc_fee(*dest_payment).unwrap(),
                ))
            }),
        );
    }

    let mut commits = Vec::new();
    for _ in 0..multi_route_choice.len() {
        let (output, _fut_index, new_fut_list) = await!(select_all(fut_list));
        match output {
            Ok(commit) => commits.push(commit),
            Err(_) => {
                let _ = await!(app_buyer.request_close_payment(payment_id));
                return Err(BuyerError::CreateTransactionFailed);
            }
        }
        fut_list = new_fut_list;
    }

    let _ = await!(app_buyer.request_close_payment(payment_id));

    let multi_commit = MultiCommit {
        invoice_id: invoice.invoice_id.clone(),
        total_dest_payment: invoice.dest_payment,
        commits,
    };

    writeln!(writer, "Payment successful!").map_err(|_| BuyerError::WriteError)?;

    // Store MultiCommit to file:
    store_multi_commit_to_file(&multi_commit, &commit_file)
        .map_err(|_| BuyerError::StoreCommitError)?;

    Ok(())
}

/// Get the current status of a payment
async fn buyer_payment_status(
    payment_status_cmd: PaymentStatusCmd,
    mut app_buyer: AppBuyer,
    writer: &mut impl io::Write,
) -> Result<(), BuyerError> {
    let PaymentStatusCmd {
        payment_id,
        receipt_file,
    } = payment_status_cmd;

    // Make sure that we will be able to write the receipt
    // before we do the actual payment:
    if receipt_file.exists() {
        return Err(BuyerError::ReceiptFileAlreadyExists);
    }

    let payment_id =
        string_to_payment_id(&payment_id).map_err(|_| BuyerError::ParsePaymentIdError)?;

    let payment_status = await!(app_buyer.request_close_payment(payment_id))
        .map_err(|_| BuyerError::RequestClosePaymentError)?;

    let opt_ack_uid = match payment_status {
        PaymentStatus::PaymentNotFound => {
            writeln!(writer, "Payment could not be found").map_err(|_| BuyerError::WriteError)?;
            None
        }
        PaymentStatus::InProgress => {
            writeln!(writer, "Payment is in progress").map_err(|_| BuyerError::WriteError)?;
            None
        }
        PaymentStatus::Success((receipt, ack_uid)) => {
            writeln!(writer, "Payment succeeded. Saving receipt to file.")
                .map_err(|_| BuyerError::WriteError)?;
            // Store receipt to file:
            store_receipt_to_file(&receipt, &receipt_file)
                .map_err(|_| BuyerError::StoreReceiptError)?;
            // Note that we must save the receipt to file before we let the node discard it.
            Some(ack_uid)
        }
        PaymentStatus::Canceled(ack_uid) => {
            writeln!(writer, "Payment was canceled.").map_err(|_| BuyerError::WriteError)?;
            Some(ack_uid)
        }
    };

    if let Some(ack_uid) = opt_ack_uid {
        await!(app_buyer.ack_close_payment(payment_id, ack_uid))
            .map_err(|_| BuyerError::AckClosePaymentError)?;
    }

    Ok(())
}

pub async fn buyer(
    buyer_cmd: BuyerCmd,
    mut node_connection: NodeConnection,
    writer: &mut impl io::Write,
) -> Result<(), BuyerError> {
    // Get our local public key:
    let mut app_report = node_connection.report().clone();
    let (node_report, incoming_mutations) =
        await!(app_report.incoming_reports()).map_err(|_| BuyerError::GetReportError)?;
    // We currently don't need live updates about report mutations:
    drop(incoming_mutations);

    let local_public_key = node_report.funder_report.local_public_key.clone();

    let app_buyer = node_connection
        .buyer()
        .ok_or(BuyerError::NoBuyerPermissions)?
        .clone();

    let app_routes = node_connection
        .routes()
        .ok_or(BuyerError::NoRoutesPermissions)?
        .clone();

    match buyer_cmd {
        BuyerCmd::PayInvoice(pay_invoice_cmd) => await!(buyer_pay_invoice(
            pay_invoice_cmd,
            local_public_key,
            app_routes,
            app_buyer,
            writer,
        ))?,
        BuyerCmd::PaymentStatus(payment_status_cmd) => {
            await!(buyer_payment_status(payment_status_cmd, app_buyer, writer,))?
        }
    }

    Ok(())
}
