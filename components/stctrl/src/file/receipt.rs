use std::fs::{self, File};
use std::io::{self, Write};
use std::path::Path;

use derive_more::*;

use app::ser_string::{
    hash_result_to_string, invoice_id_to_string, plain_lock_to_string, signature_to_string,
    string_to_hash_result, string_to_invoice_id, string_to_plain_lock, string_to_signature,
    SerStringError,
};
use app::Receipt;

use toml;

#[derive(Debug, From)]
pub enum ReceiptFileError {
    IoError(io::Error),
    TomlDeError(toml::de::Error),
    TomlSeError(toml::ser::Error),
    SerStringError,
    ParseDestPaymentError,
    ParseTotalDestPaymentError,
    InvalidPublicKey,
}

/// A helper structure for serialize and deserializing Receipt.
#[derive(Serialize, Deserialize)]
pub struct ReceiptFile {
    pub response_hash: String,
    pub invoice_id: String,
    pub src_plain_lock: String,
    pub dest_plain_lock: String,
    pub dest_payment: String,
    pub total_dest_payment: String,
    pub signature: String,
}

impl From<SerStringError> for ReceiptFileError {
    fn from(_e: SerStringError) -> Self {
        ReceiptFileError::SerStringError
    }
}

/// Load Receipt from a file
pub fn load_receipt_from_file(path: &Path) -> Result<Receipt, ReceiptFileError> {
    let data = fs::read_to_string(&path)?;
    let receipt_file: ReceiptFile = toml::from_str(&data)?;

    let response_hash = string_to_hash_result(&receipt_file.response_hash)?;
    let invoice_id = string_to_invoice_id(&receipt_file.invoice_id)?;
    let src_plain_lock = string_to_plain_lock(&receipt_file.src_plain_lock)?;
    let dest_plain_lock = string_to_plain_lock(&receipt_file.dest_plain_lock)?;

    let dest_payment = receipt_file
        .dest_payment
        .parse()
        .map_err(|_| ReceiptFileError::ParseDestPaymentError)?;

    let total_dest_payment = receipt_file
        .total_dest_payment
        .parse()
        .map_err(|_| ReceiptFileError::ParseTotalDestPaymentError)?;
    let signature = string_to_signature(&receipt_file.signature)?;

    Ok(Receipt {
        response_hash,
        invoice_id,
        src_plain_lock,
        dest_plain_lock,
        dest_payment,
        total_dest_payment,
        signature,
    })
}

/// Store Receipt to file
pub fn store_receipt_to_file(receipt: &Receipt, path: &Path) -> Result<(), ReceiptFileError> {
    let Receipt {
        ref response_hash,
        ref invoice_id,
        ref src_plain_lock,
        ref dest_plain_lock,
        dest_payment,
        total_dest_payment,
        ref signature,
    } = receipt;

    let receipt_file = ReceiptFile {
        response_hash: hash_result_to_string(&response_hash),
        invoice_id: invoice_id_to_string(&invoice_id),
        src_plain_lock: plain_lock_to_string(&src_plain_lock),
        dest_plain_lock: plain_lock_to_string(&dest_plain_lock),
        dest_payment: dest_payment.to_string(),
        total_dest_payment: total_dest_payment.to_string(),
        signature: signature_to_string(&signature),
    };

    let data = toml::to_string(&receipt_file)?;

    let mut file = File::create(path)?;
    file.write_all(&data.as_bytes())?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    use app::invoice::{InvoiceId, INVOICE_ID_LEN};
    use app::{HashResult, PlainLock, Signature, HASH_RESULT_LEN, PLAIN_LOCK_LEN, SIGNATURE_LEN};

    #[test]
    fn test_receipt_file_basic() {
        let receipt_file: ReceiptFile = toml::from_str(
            r#"
            response_hash = 'response_hash'
            invoice_id = 'invoice_id'
            src_plain_lock = 'src_plain_lock'
            dest_plain_lock = 'dest_plain_lock'
            dest_payment = '100'
            total_dest_payment = '200'
            signature = 'signature'
        "#,
        )
        .unwrap();

        assert_eq!(receipt_file.response_hash, "response_hash");
        assert_eq!(receipt_file.invoice_id, "invoice_id");
        assert_eq!(receipt_file.dest_payment, "100");
        assert_eq!(receipt_file.signature, "signature");
    }

    #[test]
    fn test_store_load_receipt() {
        // Create a temporary directory:
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("receipt_file");

        let receipt = Receipt {
            response_hash: HashResult::from(&[0; HASH_RESULT_LEN]),
            invoice_id: InvoiceId::from(&[1; INVOICE_ID_LEN]),
            src_plain_lock: PlainLock::from(&[2; PLAIN_LOCK_LEN]),
            dest_plain_lock: PlainLock::from(&[3; PLAIN_LOCK_LEN]),
            dest_payment: 100,
            total_dest_payment: 200,
            signature: Signature::from(&[4; SIGNATURE_LEN]),
        };

        store_receipt_to_file(&receipt, &file_path).unwrap();
        let receipt2 = load_receipt_from_file(&file_path).unwrap();

        assert_eq!(receipt, receipt2);
    }
}
