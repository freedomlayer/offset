use std::io;
use std::path::PathBuf;

use structopt::StructOpt;

use crate::file::invoice::load_invoice_from_file;
use crate::file::receipt::load_receipt_from_file;
use crate::file::token::load_token_from_file;

use app::ser_string::public_key_to_string;
use app::{verify_move_token_hashed_report, verify_receipt};

#[derive(Debug)]
pub enum StVerifyError {
    LoadTokenError,
    WriteError,
    TokenInvalid,
    LoadInvoiceError,
    LoadReceiptError,
    InvoiceIdMismatch,
    DestPaymentMismatch,
    InvalidReceipt,
}

/// Verify a token received from a friend.
/// A token is some recent commitment of a friend to the mutual credit balance.
#[derive(Clone, Debug, StructOpt)]
pub struct VerifyTokenCmd {
    /// Path of token file
    #[structopt(parse(from_os_str), short = "t", long = "token")]
    pub token: PathBuf,
}

/// Verify receipt file
#[derive(Clone, Debug, StructOpt)]
pub struct VerifyReceiptCmd {
    /// Path of invoice file (Locally generated)
    #[structopt(parse(from_os_str), short = "i", long = "invoice")]
    pub invoice: PathBuf,
    /// Path of receipt file (Received from buyer)
    #[structopt(parse(from_os_str), short = "r", long = "receipt")]
    pub receipt: PathBuf,
}

/// stctrl: offST ConTRoL
/// An application used to interface with the Offst node
/// Allows to view node's state information, configure node's state and send funds to remote nodes.
#[derive(Clone, Debug, StructOpt)]
#[structopt(name = "stverify")]
pub enum StVerifyCmd {
    /// Verify friend's last token
    #[structopt(name = "verify-token")]
    VerifyToken(VerifyTokenCmd),
    /// Verify a receipt against an invoice
    #[structopt(name = "verify-receipt")]
    VerifyReceipt(VerifyReceiptCmd),
}

/// Verify a given friend token
/// If the given token is valid, output token details
fn stverify_verify_token(
    verify_token_cmd: VerifyTokenCmd,
    writer: &mut impl io::Write,
) -> Result<(), StVerifyError> {
    let move_token_hashed_report =
        load_token_from_file(&verify_token_cmd.token).map_err(|_| StVerifyError::LoadTokenError)?;

    if verify_move_token_hashed_report(
        &move_token_hashed_report,
        &move_token_hashed_report.local_public_key,
    ) {
        writeln!(writer, "Token is valid!").map_err(|_| StVerifyError::WriteError)?;
        writeln!(writer).map_err(|_| StVerifyError::WriteError)?;
        writeln!(
            writer,
            "local_public_key: {}",
            public_key_to_string(&move_token_hashed_report.local_public_key)
        )
        .map_err(|_| StVerifyError::WriteError)?;
        writeln!(
            writer,
            "remote_public_key: {}",
            public_key_to_string(&move_token_hashed_report.remote_public_key)
        )
        .map_err(|_| StVerifyError::WriteError)?;
        writeln!(
            writer,
            "inconsistency_counter: {}",
            move_token_hashed_report.inconsistency_counter
        )
        .map_err(|_| StVerifyError::WriteError)?;
        writeln!(writer, "balance: {}", move_token_hashed_report.balance)
            .map_err(|_| StVerifyError::WriteError)?;
        writeln!(
            writer,
            "move_token_counter: {}",
            move_token_hashed_report.move_token_counter
        )
        .map_err(|_| StVerifyError::WriteError)?;
        writeln!(
            writer,
            "local_pending_debt: {}",
            move_token_hashed_report.local_pending_debt
        )
        .map_err(|_| StVerifyError::WriteError)?;
        writeln!(
            writer,
            "remote_pending_debt: {}",
            move_token_hashed_report.remote_pending_debt
        )
        .map_err(|_| StVerifyError::WriteError)?;

        Ok(())
    } else {
        Err(StVerifyError::TokenInvalid)
    }
}

/// Verify a given receipt
fn stverify_verify_receipt(
    verify_receipt_cmd: VerifyReceiptCmd,
    writer: &mut impl io::Write,
) -> Result<(), StVerifyError> {
    let invoice = load_invoice_from_file(&verify_receipt_cmd.invoice)
        .map_err(|_| StVerifyError::LoadInvoiceError)?;

    let receipt = load_receipt_from_file(&verify_receipt_cmd.receipt)
        .map_err(|_| StVerifyError::LoadReceiptError)?;

    // Make sure that the invoice and receipt files match:
    // Verify invoice_id match:
    if invoice.invoice_id != receipt.invoice_id {
        return Err(StVerifyError::InvoiceIdMismatch);
    }
    // Verify dest_payment match:
    if invoice.dest_payment != receipt.total_dest_payment {
        return Err(StVerifyError::DestPaymentMismatch);
    }

    if verify_receipt(&receipt, &invoice.dest_public_key) {
        writeln!(writer, "Receipt is valid!").map_err(|_| StVerifyError::WriteError)?;
        Ok(())
    } else {
        Err(StVerifyError::InvalidReceipt)
    }
}

pub fn stverify(
    st_verify_cmd: StVerifyCmd,
    writer: &mut impl io::Write,
) -> Result<(), StVerifyError> {
    match st_verify_cmd {
        StVerifyCmd::VerifyToken(verify_token_cmd) => {
            stverify_verify_token(verify_token_cmd, writer)
        }
        StVerifyCmd::VerifyReceipt(verify_receipt_cmd) => {
            stverify_verify_receipt(verify_receipt_cmd, writer)
        }
    }
}
