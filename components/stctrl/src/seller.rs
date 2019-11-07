use std::convert::TryFrom;
use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;

use derive_more::From;

use app::common::{Commit, Currency, PublicKey};
use app::conn::{AppConn, AppSeller};
use app::gen::gen_invoice_id;
use app::ser_string::{deserialize_from_string, serialize_to_string, StringSerdeError};
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
}

async fn seller_create_invoice(
    create_invoice_cmd: CreateInvoiceCmd,
    local_public_key: PublicKey,
    mut app_seller: AppSeller,
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

    app_seller
        .add_invoice(invoice_id.clone(), currency, amount)
        .await
        .map_err(|_| SellerError::AddInvoiceError)?;

    let mut file = File::create(invoice_path)?;
    file.write_all(&serialize_to_string(&invoice_file)?.as_bytes())?;
    Ok(())
}

async fn seller_cancel_invoice(
    cancel_invoice_cmd: CancelInvoiceCmd,
    mut app_seller: AppSeller,
) -> Result<(), SellerError> {
    let CancelInvoiceCmd { invoice_path } = cancel_invoice_cmd;

    let invoice_file: InvoiceFile = deserialize_from_string(&fs::read_to_string(&invoice_path)?)?;

    app_seller
        .cancel_invoice(invoice_file.invoice_id)
        .await
        .map_err(|_| SellerError::CancelInvoiceError)?;

    fs::remove_file(&invoice_path).map_err(|_| SellerError::RemoveInvoiceError)
}

async fn seller_commit_invoice(
    commit_invoice_cmd: CommitInvoiceCmd,
    mut app_seller: AppSeller,
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

    // HACK:
    #[allow(clippy::let_and_return)]
    let res = app_seller
        .commit_invoice(commit)
        .await
        .map_err(|_| SellerError::CommitInvoiceError);
    res
}

pub async fn seller(seller_cmd: SellerCmd, mut app_conn: AppConn) -> Result<(), SellerError> {
    // Get our local public key:
    let mut app_report = app_conn.report().clone();
    let (node_report, incoming_mutations) = app_report
        .incoming_reports()
        .await
        .map_err(|_| SellerError::GetReportError)?;
    // We currently don't need live updates about report mutations:
    drop(incoming_mutations);

    let local_public_key = node_report.funder_report.local_public_key.clone();

    let app_seller = app_conn
        .seller()
        .ok_or(SellerError::NoSellerPermissions)?
        .clone();

    match seller_cmd {
        SellerCmd::CreateInvoice(create_invoice_cmd) => {
            seller_create_invoice(create_invoice_cmd, local_public_key, app_seller).await?
        }
        SellerCmd::CancelInvoice(cancel_invoice_cmd) => {
            seller_cancel_invoice(cancel_invoice_cmd, app_seller).await?
        }

        SellerCmd::CommitInvoice(commit_invoice_cmd) => {
            seller_commit_invoice(commit_invoice_cmd, app_seller).await?
        }
    }

    Ok(())
}
