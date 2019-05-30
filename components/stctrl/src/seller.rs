use std::io;
use std::path::PathBuf;

use app::{AppSendFunds, NodeConnection, PublicKey};

use structopt::StructOpt;

/// Create invoice
#[derive(Clone, Debug, StructOpt)]
pub struct CreateInvoiceCmd {
    /// Payment recipient's public key (In base 64)
    #[structopt(short = "p", long = "pubkey")]
    pub public_key: String,
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
    NoFundsPermissions,
}

async fn seller_create_invoice(
    create_invoice_cmd: CreateInvoiceCmd,
    local_public_key: PublicKey,
    mut app_send_funds: AppSendFunds,
    writer: &mut impl io::Write,
) -> Result<(), SellerError> {
    unimplemented!();
}

pub async fn seller(
    seller_cmd: SellerCmd,
    mut node_connection: NodeConnection,
    writer: &mut impl io::Write,
) -> Result<(), SellerError> {
    // Get our local public key:
    let mut app_report = node_connection.report().clone();
    let (node_report, incoming_mutations) =
        await!(app_report.incoming_reports()).map_err(|_| SellerError::GetReportError)?;
    // We currently don't need live updates about report mutations:
    drop(incoming_mutations);

    let local_public_key = node_report.funder_report.local_public_key.clone();

    let app_send_funds = node_connection
        .send_funds()
        .ok_or(SellerError::NoFundsPermissions)?
        .clone();

    match seller_cmd {
        SellerCmd::CreateInvoice(create_invoice_cmd) => await!(seller_create_invoice(
            create_invoice_cmd,
            local_public_key,
            app_send_funds,
            writer,
        ))?,
        SellerCmd::CancelInvoice(_create_invoice_cmd) => unimplemented!(),
        SellerCmd::CommitInvoice(_commit_invoice_cmd) => unimplemented!(),
    }

    Ok(())
}
