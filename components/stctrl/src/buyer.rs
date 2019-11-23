use std::fs::{self, File};
use std::io::{self, Write};
use std::path::PathBuf;
use std::collections::HashSet;

use derive_more::From;

use futures::future::select_all;
use futures::sink::SinkExt;
use futures::stream::StreamExt;

use structopt::StructOpt;

use app::common::{Commit, PaymentStatus, PaymentStatusSuccess, PublicKey, MultiRoute, Currency, InvoiceId, PaymentId, Uid};
use app::conn::{buyer, routes, ConnPairApp, AppToAppServer, AppServerToApp, self, ResponseRoutesResult, RequestResult};
use app::gen::{gen_payment_id, gen_uid};
use app::ser_string::{deserialize_from_string, serialize_to_string, StringSerdeError};
use app::report::NodeReport;

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

async fn request_routes(conn_pair: &mut ConnPairApp, currency: Currency, dest_payment: u128, src_public_key: PublicKey,
    dest_public_key: PublicKey, opt_exclude: Option<(PublicKey, PublicKey)>) ->
        Result<Vec<MultiRoute>, BuyerError> {

    let request_routes_id = gen_uid();
    let app_request = conn::routes::request_routes(
            request_routes_id.clone(),
            currency,
            dest_payment,
            src_public_key,
            dest_public_key,
            opt_exclude);

    // Note: We never use the randomly generated `app_request_id` later. 
    // Instead, we are waiting
    // We generate it here just because we need to put some value into `app_request_id`.
    let app_to_app_server = AppToAppServer {
        app_request_id: gen_uid(),
        app_request,
    };
    conn_pair.sender.send(app_to_app_server).await.map_err(|_| BuyerError::AppRoutesError);

    // Wait until we get back response routes:
    while let Some(app_server_to_app) = conn_pair.receiver.next().await {
        if let AppServerToApp::ResponseRoutes(client_response_routes) = app_server_to_app {
            if client_response_routes.request_id == request_routes_id {
                if let ResponseRoutesResult::Success(multi_routes) = client_response_routes.result {
                    return Ok(multi_routes);
                }
            }
        }
    }
    return Err(BuyerError::AppRoutesError);
}

async fn create_payment(conn_pair: &mut ConnPairApp, payment_id: PaymentId,
    invoice_id: InvoiceId,
    currency: Currency,
    total_dest_payment: u128,
    dest_public_key: PublicKey) -> Result<(), BuyerError> {

    let app_request = conn::buyer::create_payment(
            payment_id,
            invoice_id,
            currency,
            total_dest_payment,
            dest_public_key);

    let app_request_id = gen_uid();
    let app_to_app_server = AppToAppServer {
        app_request_id: app_request_id.clone(),
        app_request,
    };

    conn_pair.sender.send(app_to_app_server).await.map_err(|_| BuyerError::CreatePaymentFailed);

    while let Some(app_server_to_app) = conn_pair.receiver.next().await {
        if let AppServerToApp::ReportMutations(report_mutations) = app_server_to_app {
            if let Some(cur_app_request_id) = report_mutations.opt_app_request_id {
                if cur_app_request_id == app_request_id {
                    return Ok(())
                }
            }
        }
    }

    return Err(BuyerError::CreatePaymentFailed);
}

/// Request to close payment, but do not wait for the payment to be closed.
async fn request_close_payment_nowait(conn_pair: &mut ConnPairApp, payment_id: PaymentId) -> Result<(), BuyerError> {

    let app_request = conn::buyer::request_close_payment(payment_id.clone());
    let app_request_id = gen_uid();
    let app_to_app_server = AppToAppServer {
        // We don't really care about app_request_id here, as we can wait on `request_id`
        // instead.
        app_request_id: app_request_id.clone(),
        app_request,
    };

    conn_pair.sender.send(app_to_app_server).await.map_err(|_| BuyerError::RequestClosePaymentError);

    while let Some(app_server_to_app) = conn_pair.receiver.next().await {
        if let AppServerToApp::ReportMutations(report_mutations) = app_server_to_app {
            if let Some(cur_app_request_id) = report_mutations.opt_app_request_id {
                if cur_app_request_id == app_request_id {
                    return Ok(())
                }
            }
        }
    }

    return Err(BuyerError::RequestClosePaymentError);
}

/// Request to close the payment, and wait for the payment to be closed.
async fn request_close_payment(conn_pair: &mut ConnPairApp, payment_id: PaymentId) -> Result<PaymentStatus, BuyerError> {

    let app_request = conn::buyer::request_close_payment(payment_id.clone());
    let app_request_id = gen_uid();
    let app_to_app_server = AppToAppServer {
        // We don't really care about app_request_id here, as we can wait on `request_id`
        // instead.
        app_request_id: app_request_id.clone(),
        app_request,
    };

    conn_pair.sender.send(app_to_app_server).await.map_err(|_| BuyerError::RequestClosePaymentError);

    while let Some(app_server_to_app) = conn_pair.receiver.next().await {
        if let AppServerToApp::ResponseClosePayment(response_close_payment) = app_server_to_app {
            if payment_id == response_close_payment.payment_id {
                return Ok(response_close_payment.status);
            }
        }
    }

    return Err(BuyerError::RequestClosePaymentError);
}

/// Request to close payment, but do not wait for the payment to be closed.
async fn ack_close_payment(conn_pair: &mut ConnPairApp, payment_id: PaymentId, ack_uid: Uid) -> Result<(), BuyerError> {

    let app_request = conn::buyer::ack_close_payment(payment_id.clone(), ack_uid.clone());
    let app_request_id = gen_uid();
    let app_to_app_server = AppToAppServer {
        // We don't really care about app_request_id here, as we can wait on `request_id`
        // instead.
        app_request_id: app_request_id.clone(),
        app_request,
    };

    conn_pair.sender.send(app_to_app_server).await.map_err(|_| BuyerError::AckClosePaymentError);

    while let Some(app_server_to_app) = conn_pair.receiver.next().await {
        if let AppServerToApp::ReportMutations(report_mutations) = app_server_to_app {
            if let Some(cur_app_request_id) = report_mutations.opt_app_request_id {
                if cur_app_request_id == app_request_id {
                    return Ok(())
                }
            }
        }
    }

    return Err(BuyerError::AckClosePaymentError);
}

/// Pay an invoice
async fn buyer_pay_invoice(
    pay_invoice_cmd: PayInvoiceCmd,
    local_public_key: PublicKey,
    mut conn_pair: ConnPairApp,
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
    let multi_routes = request_routes(
            &mut conn_pair,
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

    create_payment(
        &mut conn_pair,
        payment_id.clone(),
        invoice_file.invoice_id.clone(),
        invoice_file.currency.clone(),
        invoice_file.dest_payment,
        invoice_file.dest_public_key.clone())
        .await?;


    let mut requests = HashSet::new();
    // Create new transactions (One for every route). On the first failure cancel all
    // transactions. Succeed only if all transactions succeed.
    for (route_index, dest_payment) in &multi_route_choice {
        let route = &multi_route.routes[*route_index];

        let request_id = gen_uid();
        requests.insert(request_id.clone());
        let app_request = conn::buyer::create_transaction(
                        payment_id.clone(),
                        request_id,
                        route.route.clone(),
                        *dest_payment,
                        route.rate.calc_fee(*dest_payment).unwrap());

        let app_to_app_server = AppToAppServer {
            // We don't really care about app_request_id here, as we can wait on `request_id`
            // instead.
            app_request_id: gen_uid(),
            app_request,
        };
        conn_pair.sender.send(app_to_app_server).await.map_err(|_| BuyerError::CreateTransactionFailed);
    }

    // Signal that no new transactions will be created:
    request_close_payment_nowait(&mut conn_pair, payment_id.clone()).await?;

    // Wait for all incoming transaction responses:
    let mut opt_commit = None;
    while let Some(app_server_to_app) = conn_pair.receiver.next().await {
        if let AppServerToApp::TransactionResult(transaction_result) = app_server_to_app {
            // Make sure that we only get transaction results of transactions we have sent,
            // and that we get every transaction result only once.
            if !requests.remove(&transaction_result.request_id) {
                return Err(BuyerError::CreateTransactionFailed);
            }

            match transaction_result.result {
                RequestResult::Complete(commit) => opt_commit = Some(commit),
                RequestResult::Success => {},
                RequestResult::Failure => return Err(BuyerError::CreateTransactionFailed),
            }
        }
    }

    // We expect that some transaction returned with "Complete" signal:
    let commit = if let Some(commit) = opt_commit {
        commit
    } else {
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
    mut conn_pair: ConnPairApp,
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

    let payment_status = request_close_payment(&mut conn_pair, payment_id.clone()).await?;

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
        ack_close_payment(&mut conn_pair, payment_id, ack_uid).await?;

        // Remove payment file:
        fs::remove_file(&payment_path).map_err(|_| BuyerError::RemovePaymentError)?;
    }

    Ok(())
}

pub async fn buyer(
    buyer_cmd: BuyerCmd,
    node_report: &NodeReport,
    mut conn_pair: ConnPairApp,
    writer: &mut impl io::Write,
) -> Result<(), BuyerError> {
    // Get our local public key:
    let local_public_key = node_report.funder_report.local_public_key.clone();

    /*
    // TODO: Should be done outside?
    let app_routes = app_conn
        .routes()
        .ok_or(BuyerError::NoRoutesPermissions)?
        .clone();
    */

    match buyer_cmd {
        BuyerCmd::PayInvoice(pay_invoice_cmd) => {
            buyer_pay_invoice(
                pay_invoice_cmd,
                local_public_key,
                conn_pair,
                writer,
            )
            .await?
        }
        BuyerCmd::PaymentStatus(payment_status_cmd) => {
            buyer_payment_status(payment_status_cmd, conn_pair, writer).await?
        }
    }

    Ok(())
}
