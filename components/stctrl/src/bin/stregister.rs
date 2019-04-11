#![feature(futures_api, async_await, await_macro, arbitrary_self_types)]
#![feature(nll)]
#![feature(generators)]
#![feature(never_type)]
#![deny(trivial_numeric_casts, warnings)]
#![allow(intra_doc_link_resolution_failure)]

#[macro_use]
extern crate log;

use std::path::PathBuf;
use structopt::StructOpt;

use app::gen::gen_invoice_id;
use app::ser_string::{public_key_to_string, string_to_public_key};
use app::{verify_move_token_hashed_report, verify_receipt};

use stctrl::file::invoice::{load_invoice_from_file, store_invoice_to_file, Invoice};
use stctrl::file::receipt::load_receipt_from_file;
use stctrl::file::token::load_token_from_file;

#[derive(Debug)]
enum StRegisterError {
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
}

/// Generate invoice file
#[derive(Debug, StructOpt)]
struct GenInvoice {
    /// Payment recipient's public key (In base 64)
    #[structopt(short = "p")]
    public_key: String,
    /// Amount of credits to pay (A non negative integer)
    #[structopt(short = "a")]
    amount: u128,
    /// Path of output invoice file
    #[structopt(parse(from_os_str), short = "o")]
    output: PathBuf,
}

/// Verify receipt file
#[derive(Debug, StructOpt)]
struct VerifyReceipt {
    /// Path of invoice file (Locally generated)
    #[structopt(parse(from_os_str), short = "i")]
    invoice: PathBuf,
    /// Path of receipt file (Received from buyer)
    #[structopt(parse(from_os_str), short = "r")]
    receipt: PathBuf,
}

/// Verify a token received from a friend.
/// A token is some recent commitment of a friend to the mutual credit balance.
#[derive(Debug, StructOpt)]
struct VerifyToken {
    /// Path of token file
    #[structopt(parse(from_os_str), short = "t")]
    token: PathBuf,
}

#[derive(Debug, StructOpt)]
/// stregister - offST register
enum StRegister {
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
fn subcommand_verify_receipt(arg_verify_receipt: VerifyReceipt) -> Result<(), StRegisterError> {
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
        println!("Receipt is valid!");
        Ok(())
    } else {
        Err(StRegisterError::InvalidReceipt)
    }
}

/// Verify a given friend token
/// If the given token is valid, output token details
fn subcommand_verify_token(arg_verify_token: VerifyToken) -> Result<(), StRegisterError> {
    let move_token_hashed_report = load_token_from_file(&arg_verify_token.token)
        .map_err(|_| StRegisterError::LoadTokenError)?;

    if verify_move_token_hashed_report(
        &move_token_hashed_report,
        &move_token_hashed_report.local_public_key,
    ) {
        println!("Token is valid!");
        println!();
        println!(
            "local_public_key: {}",
            public_key_to_string(&move_token_hashed_report.local_public_key)
        );
        println!(
            "remote_public_key: {}",
            public_key_to_string(&move_token_hashed_report.remote_public_key)
        );
        println!(
            "inconsistency_counter: {}",
            move_token_hashed_report.inconsistency_counter
        );
        println!(
            "move_token_counter: {}",
            move_token_hashed_report.move_token_counter
        );
        println!(
            "local_pending_debt: {}",
            move_token_hashed_report.local_pending_debt
        );
        println!(
            "remote_pending_debt: {}",
            move_token_hashed_report.remote_pending_debt
        );
        Ok(())
    } else {
        Err(StRegisterError::TokenInvalid)
    }
}

fn run() -> Result<(), StRegisterError> {
    let st_register = StRegister::from_args();
    match st_register {
        StRegister::GenInvoice(gen_invoice) => subcommand_gen_invoice(gen_invoice),
        StRegister::VerifyReceipt(verify_receipt) => subcommand_verify_receipt(verify_receipt),
        StRegister::VerifyToken(verify_token) => subcommand_verify_token(verify_token),
    }
}

fn main() {
    if let Err(e) = run() {
        error!("error: {:?}", e);
    }
}
