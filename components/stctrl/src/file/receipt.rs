use std::fs::{self, File};
use std::io::{self, Write};
use std::path::Path;

use derive_more::*;

use app::invoice::InvoiceId;
use app::ser_string::{from_base64, from_string, to_base64, to_string, SerStringError};
use app::Receipt;
use app::{HashResult, PlainLock, Signature};

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
    #[serde(serialize_with = "to_base64", deserialize_with = "from_base64")]
    pub response_hash: HashResult,
    #[serde(serialize_with = "to_base64", deserialize_with = "from_base64")]
    pub invoice_id: InvoiceId,
    #[serde(serialize_with = "to_base64", deserialize_with = "from_base64")]
    pub src_plain_lock: PlainLock,
    #[serde(serialize_with = "to_base64", deserialize_with = "from_base64")]
    pub dest_plain_lock: PlainLock,
    #[serde(serialize_with = "to_string", deserialize_with = "from_string")]
    pub dest_payment: u128,
    #[serde(serialize_with = "to_string", deserialize_with = "from_string")]
    pub total_dest_payment: u128,
    #[serde(serialize_with = "to_base64", deserialize_with = "from_base64")]
    pub signature: Signature,
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

    Ok(Receipt {
        response_hash: receipt_file.response_hash,
        invoice_id: receipt_file.invoice_id,
        src_plain_lock: receipt_file.src_plain_lock,
        dest_plain_lock: receipt_file.dest_plain_lock,
        dest_payment: receipt_file.dest_payment,
        total_dest_payment: receipt_file.total_dest_payment,
        signature: receipt_file.signature,
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
        response_hash: response_hash.clone(),
        invoice_id: invoice_id.clone(),
        src_plain_lock: src_plain_lock.clone(),
        dest_plain_lock: dest_plain_lock.clone(),
        dest_payment: *dest_payment,
        total_dest_payment: *total_dest_payment,
        signature: signature.clone(),
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

    /*
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
    */

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
