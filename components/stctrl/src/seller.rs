use std::convert::TryFrom;
use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;

use futures::sink::SinkExt;
use futures::stream::StreamExt;

use derive_more::From;

use app::common::{Commit, Currency, PublicKey};
use app::conn::{self, AppRequest, AppServerToApp, AppToAppServer, ConnPairApp};
use app::gen::{gen_invoice_id, gen_uid};
use app::report::NodeReport;
use app::ser_utils::{deserialize_from_string, serialize_to_string, StringSerdeError};
use app::verify::verify_commit;

use crate::file::{CommitFile, InvoiceFile};

use structopt::StructOpt;

/// Create invoice
#[derive(Clone, Debug, StructOpt)]
pub struct CreateInvoiceCmd {
    /// Currency used to accept funds
    #[structopt(short = "c", long = "currency")]
    pub currency_name: String,
    /// Amount of credits to pay (A non negative integer)
    #[structopt(short = "a", long = "amount")]
    pub amount: u128,
    /// Path of output invoice file
    #[structopt(parse(from_os_str), short = "i", long = "invoice")]
    pub invoice_path: PathBuf,
}

/// Cancel invoice
#[derive(Clone, Debug, StructOpt)]
pub struct CancelInvoiceCmd {
    /// Path to invoice file
    #[structopt(parse(from_os_str), short = "i", long = "invoice")]
    pub invoice_path: PathBuf,
}

/// Commit invoice (using a Commit message from buyer)
#[derive(Clone, Debug, StructOpt)]
pub struct CommitInvoiceCmd {
    /// Path to invoice file
    #[structopt(parse(from_os_str), short = "i", long = "invoice")]
    pub invoice_path: PathBuf,
    /// Path to commit file
    #[structopt(parse(from_os_str), short = "c", long = "commit")]
    pub commit_path: PathBuf,
}

/// Funds sending related commands
#[derive(Clone, Debug, StructOpt)]
pub enum SellerCmd {
    /// Create a new invoice (to receive payment)
    #[structopt(name = "create-invoice")]
    CreateInvoice(CreateInvoiceCmd),
    /// Cancel an invoice
    #[structopt(name = "cancel-invoice")]
    CancelInvoice(CancelInvoiceCmd),
    /// Commit an invoice (Using a Commit message from buyer)
    #[structopt(name = "commit-invoice")]
    CommitInvoice(CommitInvoiceCmd),
}

#[derive(Debug, From)]
pub enum SellerError {
    GetReportError,
    NoSellerPermissions,
    ParsePublicKeyError,
    InvoiceFileAlreadyExists,
    StoreInvoiceError,
    AddInvoiceError,
    LoadInvoiceError,
    CancelInvoiceError,
    LoadCommitError,
    CommitInvoiceError,
    InvoiceCommitMismatch,
    RemoveInvoiceError,
    IoError(std::io::Error),
    StringSerdeError(StringSerdeError),
    InvalidCurrencyName,
    InvalidCommit,
    SellerRequestError,
}

async fn seller_request(
    conn_pair: &mut ConnPairApp,
    app_request: AppRequest,
) -> Result<(), SellerError> {
    let app_request_id = gen_uid();
    let app_to_app_server = AppToAppServer {
        app_request_id: app_request_id.clone(),
        app_request,
    };
    conn_pair
        .sender
        .send(app_to_app_server)
        .await
        .map_err(|_| SellerError::SellerRequestError);

    // Wait until we get an ack for our request:
    while let Some(app_server_to_app) = conn_pair.receiver.next().await {
        if let AppServerToApp::ReportMutations(report_mutations) = app_server_to_app {
            if let Some(cur_app_request_id) = report_mutations.opt_app_request_id {
                if cur_app_request_id == app_request_id {
                    return Ok(());
                }
            }
        }
    }

    Err(SellerError::SellerRequestError)
}

async fn seller_create_invoice(
    create_invoice_cmd: CreateInvoiceCmd,
    local_public_key: PublicKey,
    mut conn_pair: ConnPairApp,
) -> Result<(), SellerError> {
    let CreateInvoiceCmd {
        currency_name,
        amount,
        invoice_path,
    } = create_invoice_cmd;

    let currency =
        Currency::try_from(currency_name).map_err(|_| SellerError::InvalidCurrencyName)?;

    // Make sure we don't override an existing invoice file:
    if invoice_path.exists() {
        return Err(SellerError::InvoiceFileAlreadyExists);
    }

    let invoice_id = gen_invoice_id();

    let dest_public_key = local_public_key;

    let invoice_file = InvoiceFile {
        invoice_id: invoice_id.clone(),
        currency: currency.clone(),
        dest_public_key,
        dest_payment: amount,
    };

    seller_request(
        &mut conn_pair,
        conn::seller::add_invoice(invoice_id.clone(), currency, amount),
    )
    .await
    .map_err(|_| SellerError::AddInvoiceError)?;

    let mut file = File::create(invoice_path)?;
    file.write_all(&serialize_to_string(&invoice_file)?.as_bytes())?;
    Ok(())
}

async fn seller_cancel_invoice(
    cancel_invoice_cmd: CancelInvoiceCmd,
    mut conn_pair: ConnPairApp,
) -> Result<(), SellerError> {
    let CancelInvoiceCmd { invoice_path } = cancel_invoice_cmd;

    let invoice_file: InvoiceFile = deserialize_from_string(&fs::read_to_string(&invoice_path)?)?;

    seller_request(
        &mut conn_pair,
        conn::seller::cancel_invoice(invoice_file.invoice_id),
    )
    .await
    .map_err(|_| SellerError::CancelInvoiceError)?;

    fs::remove_file(&invoice_path).map_err(|_| SellerError::RemoveInvoiceError)
}

async fn seller_commit_invoice(
    commit_invoice_cmd: CommitInvoiceCmd,
    mut conn_pair: ConnPairApp,
) -> Result<(), SellerError> {
    let CommitInvoiceCmd {
        invoice_path,
        commit_path,
    } = commit_invoice_cmd;

    // Note: We don't really need the invoice for the internal API.
    // We require it here to enforce the user to understand that the commit file corresponds to a
    // certain invoice file.

    let invoice_file: InvoiceFile = deserialize_from_string(&fs::read_to_string(&invoice_path)?)?;

    let commit_file: CommitFile = deserialize_from_string(&fs::read_to_string(&commit_path)?)?;
    let commit = Commit::from(commit_file);

    if !verify_commit(&commit, &invoice_file.dest_public_key) {
        return Err(SellerError::InvalidCommit);
    }

    if commit.invoice_id != invoice_file.invoice_id {
        return Err(SellerError::InvoiceCommitMismatch);
    }

    seller_request(&mut conn_pair, conn::seller::commit_invoice(commit))
        .await
        .map_err(|_| SellerError::CommitInvoiceError)
}

pub async fn seller(
    seller_cmd: SellerCmd,
    node_report: &NodeReport,
    conn_pair: ConnPairApp,
) -> Result<(), SellerError> {
    // Get our local public key:
    let local_public_key = node_report.funder_report.local_public_key.clone();

    /*
    // TODO; Check permissions at caller site.
    let app_seller = app_conn
        .seller()
        .ok_or(SellerError::NoSellerPermissions)?
        .clone();
    */

    match seller_cmd {
        SellerCmd::CreateInvoice(create_invoice_cmd) => {
            seller_create_invoice(create_invoice_cmd, local_public_key, conn_pair).await?
        }
        SellerCmd::CancelInvoice(cancel_invoice_cmd) => {
            seller_cancel_invoice(cancel_invoice_cmd, conn_pair).await?
        }
        SellerCmd::CommitInvoice(commit_invoice_cmd) => {
            seller_commit_invoice(commit_invoice_cmd, conn_pair).await?
        }
    }

    Ok(())
}
