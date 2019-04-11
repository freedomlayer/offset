use std::io;
use std::path::PathBuf;
use structopt::StructOpt;

use app::gen::gen_invoice_id;
use app::ser_string::{public_key_to_string, string_to_public_key};
use app::{verify_move_token_hashed_report, verify_receipt};

use crate::file::invoice::{load_invoice_from_file, store_invoice_to_file, Invoice};
use crate::file::receipt::load_receipt_from_file;
use crate::file::token::load_token_from_file;

#[derive(Debug)]
pub enum StRegisterError {
    InvoiceFileAlreadyExists,
    StoreInvoiceError,
    LoadInvoiceError,
    LoadReceiptError,
    DestPaymentMismatch,
    InvoiceIdMismatch,
    InvalidReceipt,
    ParsePublicKeyError,
    LoadTokenError,
    TokenInvalid,
    WriteError,
}

/// Generate invoice file
#[derive(Clone, Debug, StructOpt)]
pub struct GenInvoice {
    /// Payment recipient's public key (In base 64)
    #[structopt(short = "p")]
    pub public_key: String,
    /// Amount of credits to pay (A non negative integer)
    #[structopt(short = "a")]
    pub amount: u128,
    /// Path of output invoice file
    #[structopt(parse(from_os_str), short = "o")]
    pub output: PathBuf,
}

/// Verify receipt file
#[derive(Clone, Debug, StructOpt)]
pub struct VerifyReceipt {
    /// Path of invoice file (Locally generated)
    #[structopt(parse(from_os_str), short = "i")]
    pub invoice: PathBuf,
    /// Path of receipt file (Received from buyer)
    #[structopt(parse(from_os_str), short = "r")]
    pub receipt: PathBuf,
}

/// Verify a token received from a friend.
/// A token is some recent commitment of a friend to the mutual credit balance.
#[derive(Clone, Debug, StructOpt)]
pub struct VerifyToken {
    /// Path of token file
    #[structopt(parse(from_os_str), short = "t")]
    pub token: PathBuf,
}

#[derive(Clone, Debug, StructOpt)]
/// stregister - offST register
pub enum StRegisterCmd {
    #[structopt(name = "gen-invoice")]
    GenInvoice(GenInvoice),
    #[structopt(name = "verify-receipt")]
    VerifyReceipt(VerifyReceipt),
    #[structopt(name = "verify-token")]
    VerifyToken(VerifyToken),
}

/// Randomly generate an invoice and store it to an output file
fn subcommand_gen_invoice(arg_gen_invoice: GenInvoice) -> Result<(), StRegisterError> {
    let invoice_id = gen_invoice_id();

    let dest_public_key = string_to_public_key(&arg_gen_invoice.public_key)
        .map_err(|_| StRegisterError::ParsePublicKeyError)?;

    let invoice = Invoice {
        invoice_id,
        dest_public_key,
        dest_payment: arg_gen_invoice.amount,
    };

    // Make sure we don't override an existing invoice file:
    if arg_gen_invoice.output.exists() {
        return Err(StRegisterError::InvoiceFileAlreadyExists);
    }

    store_invoice_to_file(&invoice, &arg_gen_invoice.output)
        .map_err(|_| StRegisterError::StoreInvoiceError)
}

/// Verify a given receipt
fn subcommand_verify_receipt(
    arg_verify_receipt: VerifyReceipt,
    writer: &mut impl io::Write,
) -> Result<(), StRegisterError> {
    let invoice = load_invoice_from_file(&arg_verify_receipt.invoice)
        .map_err(|_| StRegisterError::LoadInvoiceError)?;

    let receipt = load_receipt_from_file(&arg_verify_receipt.receipt)
        .map_err(|_| StRegisterError::LoadReceiptError)?;

    // Make sure that the invoice and receipt files match:
    // Verify invoice_id match:
    if invoice.invoice_id != receipt.invoice_id {
        return Err(StRegisterError::InvoiceIdMismatch);
    }
    // Verify dest_payment match:
    if invoice.dest_payment != receipt.dest_payment {
        return Err(StRegisterError::DestPaymentMismatch);
    }

    if verify_receipt(&receipt, &invoice.dest_public_key) {
        writeln!(writer, "Receipt is valid!").map_err(|_| StRegisterError::WriteError)?;
        Ok(())
    } else {
        Err(StRegisterError::InvalidReceipt)
    }
}

/// Verify a given friend token
/// If the given token is valid, output token details
fn subcommand_verify_token(
    arg_verify_token: VerifyToken,
    writer: &mut impl io::Write,
) -> Result<(), StRegisterError> {
    let move_token_hashed_report = load_token_from_file(&arg_verify_token.token)
        .map_err(|_| StRegisterError::LoadTokenError)?;

    if verify_move_token_hashed_report(
        &move_token_hashed_report,
        &move_token_hashed_report.local_public_key,
    ) {
        writeln!(writer, "Token is valid!").map_err(|_| StRegisterError::WriteError)?;
        writeln!(writer, "").map_err(|_| StRegisterError::WriteError)?;
        writeln!(
            writer,
            "local_public_key: {}",
            public_key_to_string(&move_token_hashed_report.local_public_key)
        )
        .map_err(|_| StRegisterError::WriteError)?;
        writeln!(
            writer,
            "remote_public_key: {}",
            public_key_to_string(&move_token_hashed_report.remote_public_key)
        )
        .map_err(|_| StRegisterError::WriteError)?;
        writeln!(
            writer,
            "inconsistency_counter: {}",
            move_token_hashed_report.inconsistency_counter
        )
        .map_err(|_| StRegisterError::WriteError)?;
        writeln!(
            writer,
            "move_token_counter: {}",
            move_token_hashed_report.move_token_counter
        )
        .map_err(|_| StRegisterError::WriteError)?;
        writeln!(
            writer,
            "local_pending_debt: {}",
            move_token_hashed_report.local_pending_debt
        )
        .map_err(|_| StRegisterError::WriteError)?;
        writeln!(
            writer,
            "remote_pending_debt: {}",
            move_token_hashed_report.remote_pending_debt
        )
        .map_err(|_| StRegisterError::WriteError)?;

        Ok(())
    } else {
        Err(StRegisterError::TokenInvalid)
    }
}

pub fn stregister(
    st_register_cmd: StRegisterCmd,
    writer: &mut impl io::Write,
) -> Result<(), StRegisterError> {
    match st_register_cmd {
        StRegisterCmd::GenInvoice(gen_invoice) => subcommand_gen_invoice(gen_invoice),
        StRegisterCmd::VerifyReceipt(verify_receipt) => {
            subcommand_verify_receipt(verify_receipt, writer)
        }
        StRegisterCmd::VerifyToken(verify_token) => subcommand_verify_token(verify_token, writer),
    }
}
