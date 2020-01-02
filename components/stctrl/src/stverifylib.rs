use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;

use derive_more::From;

use structopt::StructOpt;

use crate::file::{InvoiceFile, ReceiptFile, TokenFile};

use app::common::Receipt;
use app::report::MoveTokenHashedReport;
use app::ser_utils::{deserialize_from_string, public_key_to_string, StringSerdeError};
use app::verify::{verify_move_token_hashed_report, verify_receipt};

#[derive(Debug, From)]
pub enum StVerifyError {
    LoadTokenError,
    WriteError,
    TokenInvalid,
    LoadInvoiceError,
    LoadReceiptError,
    InvoiceIdMismatch,
    DestPaymentMismatch,
    InvalidReceipt,
    IoError(std::io::Error),
    StringSerdeError(StringSerdeError),
}

/// Verify a token received from a friend.
/// A token is some recent commitment of a friend to the mutual credit balance.
#[derive(Clone, Debug, StructOpt)]
pub struct VerifyTokenCmd {
    /// Path of token file
    #[structopt(parse(from_os_str), short = "t", long = "token")]
    pub token_path: PathBuf,
}

/// Verify receipt file
#[derive(Clone, Debug, StructOpt)]
pub struct VerifyReceiptCmd {
    /// Path of invoice file (Locally generated)
    #[structopt(parse(from_os_str), short = "i", long = "invoice")]
    pub invoice_path: PathBuf,
    /// Path of receipt file (Received from buyer)
    #[structopt(parse(from_os_str), short = "r", long = "receipt")]
    pub receipt_path: PathBuf,
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
    let token_file: TokenFile =
        deserialize_from_string(&fs::read_to_string(&verify_token_cmd.token_path)?)?;

    let move_token_hashed_report = MoveTokenHashedReport::from(token_file);

    if verify_move_token_hashed_report(
        &move_token_hashed_report,
        &move_token_hashed_report.token_info.mc.local_public_key,
    ) {
        writeln!(writer, "Token is valid!").map_err(|_| StVerifyError::WriteError)?;
        writeln!(writer).map_err(|_| StVerifyError::WriteError)?;
        writeln!(
            writer,
            "local_public_key: {}",
            public_key_to_string(&move_token_hashed_report.token_info.mc.local_public_key)
        )
        .map_err(|_| StVerifyError::WriteError)?;
        writeln!(
            writer,
            "remote_public_key: {}",
            public_key_to_string(&move_token_hashed_report.token_info.mc.remote_public_key)
        )
        .map_err(|_| StVerifyError::WriteError)?;
        writeln!(
            writer,
            "inconsistency_counter: {}",
            move_token_hashed_report
                .token_info
                .counters
                .inconsistency_counter
        )
        .map_err(|_| StVerifyError::WriteError)?;
        writeln!(
            writer,
            "move_token_counter: {}",
            move_token_hashed_report
                .token_info
                .counters
                .move_token_counter
        )
        .map_err(|_| StVerifyError::WriteError)?;

        writeln!(writer, "balances:\n");

        for currency_balance_info in move_token_hashed_report.token_info.mc.balances {
            let balance_info = &currency_balance_info.balance_info;
            writeln!(
                writer,
                "- {}: balance={}, lpd={}, rpd={}",
                currency_balance_info.currency,
                balance_info.balance,
                balance_info.local_pending_debt,
                balance_info.remote_pending_debt
            )
            .map_err(|_| StVerifyError::WriteError)?;
        }

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
    let invoice_file: InvoiceFile =
        deserialize_from_string(&fs::read_to_string(&verify_receipt_cmd.invoice_path)?)?;

    let receipt_file: ReceiptFile =
        deserialize_from_string(&fs::read_to_string(&verify_receipt_cmd.receipt_path)?)?;
    let receipt = Receipt::from(receipt_file);

    // Make sure that the invoice and receipt files match:
    // Verify invoice_id match:
    if invoice_file.invoice_id != receipt.invoice_id {
        return Err(StVerifyError::InvoiceIdMismatch);
    }
    // Verify dest_payment match:
    if invoice_file.dest_payment != receipt.total_dest_payment {
        return Err(StVerifyError::DestPaymentMismatch);
    }

    if verify_receipt(&receipt, &invoice_file.dest_public_key) {
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
