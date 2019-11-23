use std::fs::{self, File};
use std::io::{self, Write};
use std::path::PathBuf;

use derive_more::From;

use futures::future::select_all;

use structopt::StructOpt;

use app::common::{Commit, PaymentStatus, PaymentStatusSuccess, PublicKey};
use app::conn::{buyer, routes};
use app::gen::{gen_payment_id, gen_uid};
use app::ser_string::{deserialize_from_string, serialize_to_string, StringSerdeError};

use crate::file::{CommitFile, InvoiceFile, PaymentFile, ReceiptFile};

use crate::multi_route_util::choose_multi_route;

/// Pay an invoice
#[derive(Clone, Debug, StructOpt)]
pub struct PayInvoiceCmd {
    /// Path to invoice file to pay
    #[structopt(parse(from_os_str), short = "i", long = "invoice")]
    pub invoice_path: PathBuf,
    /// Output payment file (Used to track the payment)
    #[structopt(parse(from_os_str), short = "p", long = "payment")]
    pub payment_path: PathBuf,
    /// Output commit file
    #[structopt(parse(from_os_str), short = "c", long = "commit")]
    pub commit_path: PathBuf,
}

/// Check payment status (And obtain receipt if successful)
#[derive(Clone, Debug, StructOpt)]
pub struct PaymentStatusCmd {
    /// Payment file
    #[structopt(parse(from_os_str), short = "p", long = "payment")]
    pub payment_path: PathBuf,
    /// Output receipt file (in case a receipt is ready)
    #[structopt(parse(from_os_str), short = "r", long = "receipt")]
    pub receipt_path: PathBuf,
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

#[derive(Debug, From)]
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
    PaymentFileAlreadyExists,
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
    StorePaymentError,
    LoadPaymentError,
    RemovePaymentError,
    PaymentIncomplete,
    IoError(std::io::Error),
    StringSerdeError(StringSerdeError),
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
        invoice_path,
        payment_path,
        commit_path,
    } = pay_invoice_cmd;

    // Make sure that we will be able to write the Payment file
    // before we do the actual payment:
    if payment_path.exists() {
        return Err(BuyerError::PaymentFileAlreadyExists);
    }

    // Make sure that we will be able to write the Commit
    // before we do the actual payment:
    if commit_path.exists() {
        return Err(BuyerError::CommitFileAlreadyExists);
    }

    let invoice_file: InvoiceFile = deserialize_from_string(&fs::read_to_string(&invoice_path)?)?;

    // TODO: We might get routes with the exact capacity,
    // but this will not be enough for sending our amount because
    // we also need to pay nodes on the way.
    // We might need to solve this issue at the index server side
    // (Should the Server take into account the extra credits that should be paid along the way?).
    let multi_routes = app_routes
        .request_routes(
            invoice_file.currency.clone(),
            invoice_file.dest_payment,
            local_public_key, // source
            invoice_file.dest_public_key.clone(),
            None,
        )
        .await // No exclusion of edges
        .map_err(|_| BuyerError::AppRoutesError)?;

    let (route_index, multi_route_choice) =
        choose_multi_route(&multi_routes, invoice_file.dest_payment)
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
    let payment_file = PaymentFile {
        payment_id: payment_id.clone(),
    };

    // Keep payment id for later reference:
    let mut file = File::create(payment_path)?;
    file.write_all(&serialize_to_string(&payment_file)?.as_bytes())?;

    app_buyer
        .create_payment(
            payment_id.clone(),
            invoice_file.invoice_id.clone(),
            invoice_file.currency.clone(),
            invoice_file.dest_payment,
            invoice_file.dest_public_key.clone(),
        )
        .await
        .map_err(|_| BuyerError::CreatePaymentFailed)?;

    // Create new transactions (One for every route). On the first failure cancel all
    // transactions. Succeed only if all transactions succeed.

    let mut fut_list = Vec::new();
    for (route_index, dest_payment) in &multi_route_choice {
        let route = &multi_route.routes[*route_index];
        let request_id = gen_uid();
        let mut c_app_buyer = app_buyer.clone();
        let c_payment_id = payment_id.clone();
        fut_list.push(
            // TODO: Possibly a more efficient way than Box::pin?
            Box::pin(async move {
                c_app_buyer
                    .create_transaction(
                        c_payment_id,
                        request_id,
                        route.route.clone(),
                        *dest_payment,
                        route.rate.calc_fee(*dest_payment).unwrap(),
                    )
                    .await
            }),
        );
    }

    let mut opt_commit = None;
    for _ in 0..multi_route_choice.len() {
        let (output, _fut_index, new_fut_list) = select_all(fut_list).await;
        match output {
            Ok(Some(commit)) => opt_commit = Some(commit),
            Ok(None) => {}
            Err(_) => {
                let _ = app_buyer.request_close_payment(payment_id.clone()).await;
                return Err(BuyerError::CreateTransactionFailed);
            }
        }
        fut_list = new_fut_list;
    }

    // TODO: We are not attempting to close the payment here, because this is going to wait
    // until a receipt is received, but a receipt will never be received because we haven't
    // yet produced a commit message.
    //
    // Possibly add some kind of nonblocking-close-payment method to app_buyer in the future?
    // let _ = app_buyer.request_close_payment(payment_id).await;

    let commit = if let Some(commit) = opt_commit {
        commit
    } else {
        // For some reason we never got back a "is_complete=true" signal.
        return Err(BuyerError::PaymentIncomplete);
    };

    writeln!(writer, "Payment successful!").map_err(|_| BuyerError::WriteError)?;

    let commit_file = CommitFile::from(commit);

    // Store Commit to file:
    let mut file = File::create(commit_path)?;
    file.write_all(&serialize_to_string(&commit_file)?.as_bytes())?;

    Ok(())
}

/// Get the current status of a payment
async fn buyer_payment_status(
    payment_status_cmd: PaymentStatusCmd,
    mut app_buyer: AppBuyer,
    writer: &mut impl io::Write,
) -> Result<(), BuyerError> {
    let PaymentStatusCmd {
        payment_path,
        receipt_path,
    } = payment_status_cmd;

    // Make sure that we will be able to write the receipt
    // before we do the actual payment:
    if receipt_path.exists() {
        return Err(BuyerError::ReceiptFileAlreadyExists);
    }

    let payment_file: PaymentFile = deserialize_from_string(&fs::read_to_string(&payment_path)?)?;
    let payment_id = payment_file.payment_id;

    let payment_status = app_buyer
        .request_close_payment(payment_id.clone())
        .await
        .map_err(|_| BuyerError::RequestClosePaymentError)?;

    let opt_ack_uid = match payment_status {
        PaymentStatus::PaymentNotFound => {
            writeln!(writer, "Payment could not be found").map_err(|_| BuyerError::WriteError)?;
            // Remove payment file:
            fs::remove_file(&payment_path).map_err(|_| BuyerError::RemovePaymentError)?;
            None
        }
        PaymentStatus::Success(PaymentStatusSuccess { receipt, ack_uid }) => {
            writeln!(writer, "Payment succeeded. Saving receipt to file.")
                .map_err(|_| BuyerError::WriteError)?;

            // Store receipt to file:
            let mut file = File::create(receipt_path)?;
            file.write_all(&serialize_to_string(&ReceiptFile::from(receipt))?.as_bytes())?;

            // Note that we must save the receipt to file before we let the node discard it.
            Some(ack_uid)
        }
        PaymentStatus::Canceled(ack_uid) => {
            writeln!(writer, "Payment was canceled.").map_err(|_| BuyerError::WriteError)?;

            Some(ack_uid)
        }
    };

    if let Some(ack_uid) = opt_ack_uid {
        app_buyer
            .ack_close_payment(payment_id, ack_uid)
            .await
            .map_err(|_| BuyerError::AckClosePaymentError)?;

        // Remove payment file:
        fs::remove_file(&payment_path).map_err(|_| BuyerError::RemovePaymentError)?;
    }

    Ok(())
}

pub async fn buyer(
    buyer_cmd: BuyerCmd,
    mut app_conn: AppConn,
    writer: &mut impl io::Write,
) -> Result<(), BuyerError> {
    // Get our local public key:
    let mut app_report = app_conn.report().clone();
    let (node_report, incoming_mutations) = app_report
        .incoming_reports()
        .await
        .map_err(|_| BuyerError::GetReportError)?;
    // We currently don't need live updates about report mutations:
    drop(incoming_mutations);

    let local_public_key = node_report.funder_report.local_public_key.clone();

    let app_buyer = app_conn
        .buyer()
        .ok_or(BuyerError::NoBuyerPermissions)?
        .clone();

    let app_routes = app_conn
        .routes()
        .ok_or(BuyerError::NoRoutesPermissions)?
        .clone();

    match buyer_cmd {
        BuyerCmd::PayInvoice(pay_invoice_cmd) => {
            buyer_pay_invoice(
                pay_invoice_cmd,
                local_public_key,
                app_routes,
                app_buyer,
                writer,
            )
            .await?
        }
        BuyerCmd::PaymentStatus(payment_status_cmd) => {
            buyer_payment_status(payment_status_cmd, app_buyer, writer).await?
        }
    }

    Ok(())
}
