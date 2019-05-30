use std::path::PathBuf;

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
pub enum SellerError {}
