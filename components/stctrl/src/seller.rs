use std::path::PathBuf;

use app::gen::gen_invoice_id;
use app::{AppSeller, NodeConnection, PublicKey};

use crate::file::invoice::{load_invoice_from_file, store_invoice_to_file, Invoice};
use crate::file::multi_commit::load_multi_commit_from_file;

use structopt::StructOpt;

/// Create invoice
#[derive(Clone, Debug, StructOpt)]
pub struct CreateInvoiceCmd {
    /// Amount of credits to pay (A non negative integer)
    #[structopt(short = "a", long = "amount")]
    pub amount: u128,
    /// Path of output invoice file
    #[structopt(parse(from_os_str), short = "o", long = "output")]
    pub output: PathBuf,
}

/// Cancel invoice
#[derive(Clone, Debug, StructOpt)]
pub struct CancelInvoiceCmd {
    /// Path to invoice file
    #[structopt(parse(from_os_str), short = "i", long = "invoice")]
    pub invoice_file: PathBuf,
}

/// Commit invoice (using a Commit message from buyer)
#[derive(Clone, Debug, StructOpt)]
pub struct CommitInvoiceCmd {
    /// Path to invoice file
    #[structopt(parse(from_os_str), short = "i", long = "invoice")]
    pub invoice_file: PathBuf,
    /// Path to commit file
    #[structopt(parse(from_os_str), short = "c", long = "commit")]
    pub commit_file: PathBuf,
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

#[derive(Debug)]
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
}

async fn seller_create_invoice(
    create_invoice_cmd: CreateInvoiceCmd,
    local_public_key: PublicKey,
    mut app_seller: AppSeller,
) -> Result<(), SellerError> {
    let CreateInvoiceCmd { amount, output } = create_invoice_cmd;

    // Make sure we don't override an existing invoice file:
    if output.exists() {
        return Err(SellerError::InvoiceFileAlreadyExists);
    }

    let invoice_id = gen_invoice_id();

    let dest_public_key = local_public_key;

    let invoice = Invoice {
        invoice_id: invoice_id.clone(),
        dest_public_key,
        dest_payment: amount,
    };

    await!(app_seller.add_invoice(invoice_id.clone(), amount))
        .map_err(|_| SellerError::AddInvoiceError)?;

    store_invoice_to_file(&invoice, &output).map_err(|_| SellerError::StoreInvoiceError)
}

async fn seller_cancel_invoice(
    cancel_invoice_cmd: CancelInvoiceCmd,
    mut app_seller: AppSeller,
) -> Result<(), SellerError> {
    let CancelInvoiceCmd { invoice_file } = cancel_invoice_cmd;

    let invoice =
        load_invoice_from_file(&invoice_file).map_err(|_| SellerError::LoadInvoiceError)?;

    await!(app_seller.cancel_invoice(invoice.invoice_id))
        .map_err(|_| SellerError::CancelInvoiceError)
}

async fn seller_commit_invoice(
    commit_invoice_cmd: CommitInvoiceCmd,
    mut app_seller: AppSeller,
) -> Result<(), SellerError> {
    let CommitInvoiceCmd {
        invoice_file,
        commit_file,
    } = commit_invoice_cmd;

    // Note: We don't really need the invoice for the internal API.
    // We require it here to enforce the user to understand that the commit file corresponds to a
    // certain invoice file.
    let invoice =
        load_invoice_from_file(&invoice_file).map_err(|_| SellerError::LoadInvoiceError)?;

    let multi_commit =
        load_multi_commit_from_file(&commit_file).map_err(|_| SellerError::LoadCommitError)?;

    if multi_commit.invoice_id != invoice.invoice_id {
        return Err(SellerError::InvoiceCommitMismatch);
    }

    await!(app_seller.commit_invoice(multi_commit)).map_err(|_| SellerError::CommitInvoiceError)
}

pub async fn seller(
    seller_cmd: SellerCmd,
    mut node_connection: NodeConnection,
) -> Result<(), SellerError> {
    // Get our local public key:
    let mut app_report = node_connection.report().clone();
    let (node_report, incoming_mutations) =
        await!(app_report.incoming_reports()).map_err(|_| SellerError::GetReportError)?;
    // We currently don't need live updates about report mutations:
    drop(incoming_mutations);

    let local_public_key = node_report.funder_report.local_public_key.clone();

    let app_seller = node_connection
        .seller()
        .ok_or(SellerError::NoSellerPermissions)?
        .clone();

    match seller_cmd {
        SellerCmd::CreateInvoice(create_invoice_cmd) => await!(seller_create_invoice(
            create_invoice_cmd,
            local_public_key,
            app_seller,
        ))?,
        SellerCmd::CancelInvoice(cancel_invoice_cmd) => {
            await!(seller_cancel_invoice(cancel_invoice_cmd, app_seller))?
        }

        SellerCmd::CommitInvoice(commit_invoice_cmd) => {
            await!(seller_commit_invoice(commit_invoice_cmd, app_seller))?
        }
    }

    Ok(())
}
