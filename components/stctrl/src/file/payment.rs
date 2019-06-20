use std::fs::{self, File};
use std::io::{self, Write};
use std::path::Path;

use derive_more::*;

use app::ser_string::{from_base64, to_base64, SerStringError};

use app::payment::PaymentId;

use toml;

#[derive(Debug, PartialEq, Eq)]
pub struct Payment {
    pub payment_id: PaymentId,
}

#[derive(Debug, From)]
pub enum PaymentFileError {
    IoError(io::Error),
    TomlDeError(toml::de::Error),
    TomlSeError(toml::ser::Error),
    SerStringError,
    ParseDestPaymentError,
    InvalidPublicKey,
}

/// A helper structure for serialize and deserializing Payment.
#[derive(Serialize, Deserialize)]
pub struct PaymentFile {
    #[serde(serialize_with = "to_base64", deserialize_with = "from_base64")]
    pub payment_id: PaymentId,
}

impl From<SerStringError> for PaymentFileError {
    fn from(_e: SerStringError) -> Self {
        PaymentFileError::SerStringError
    }
}

/// Load Payment from a file
pub fn load_payment_from_file(path: &Path) -> Result<Payment, PaymentFileError> {
    let data = fs::read_to_string(&path)?;
    let payment_file: PaymentFile = toml::from_str(&data)?;

    Ok(Payment {
        payment_id: payment_file.payment_id,
    })
}

/// Store Payment to file
pub fn store_payment_to_file(payment: &Payment, path: &Path) -> Result<(), PaymentFileError> {
    let Payment { ref payment_id } = payment;

    let payment_file = PaymentFile {
        payment_id: *payment_id,
    };

    let data = toml::to_string(&payment_file)?;

    let mut file = File::create(path)?;
    file.write_all(&data.as_bytes())?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    use app::payment::{PaymentId, PAYMENT_ID_LEN};

    /*
    #[test]
    fn test_payment_file_basic() {
        let payment_file: PaymentFile = toml::from_str(
            r#"
            payment_id = 'payment_id'
        "#,
        )
        .unwrap();

        assert_eq!(payment_file.payment_id, "payment_id");
    }
    */

    #[test]
    fn test_store_load_payment() {
        // Create a temporary directory:
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("payment_file");

        let payment = Payment {
            payment_id: PaymentId::from(&[0; PAYMENT_ID_LEN]),
        };

        store_payment_to_file(&payment, &file_path).unwrap();
        let payment2 = load_payment_from_file(&file_path).unwrap();

        assert_eq!(payment, payment2);
    }
}
