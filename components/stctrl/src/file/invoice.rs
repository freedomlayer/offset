use std::fs::{self, File};
use std::io::{self, Write};
use std::path::Path;

use derive_more::*;

use app::ser_string::{
    invoice_id_to_string, public_key_to_string, string_to_invoice_id, string_to_public_key,
    SerStringError,
};
use app::PublicKey;

use app::invoice::InvoiceId;

use toml;

#[derive(Debug, PartialEq, Eq)]
pub struct Invoice {
    pub invoice_id: InvoiceId,
    pub dest_public_key: PublicKey,
    pub dest_payment: u128,
}

#[derive(Debug, From)]
pub enum InvoiceFileError {
    IoError(io::Error),
    TomlDeError(toml::de::Error),
    TomlSeError(toml::ser::Error),
    SerStringError,
    ParseDestPaymentError,
    InvalidPublicKey,
}

/// A helper structure for serialize and deserializing Invoice.
#[derive(Serialize, Deserialize)]
pub struct InvoiceFile {
    pub invoice_id: String,
    pub dest_public_key: String,
    pub dest_payment: String,
}

impl From<SerStringError> for InvoiceFileError {
    fn from(_e: SerStringError) -> Self {
        InvoiceFileError::SerStringError
    }
}

/// Load Invoice from a file
pub fn load_invoice_from_file(path: &Path) -> Result<Invoice, InvoiceFileError> {
    let data = fs::read_to_string(&path)?;
    let invoice_file: InvoiceFile = toml::from_str(&data)?;

    let invoice_id = string_to_invoice_id(&invoice_file.invoice_id)?;
    let dest_public_key = string_to_public_key(&invoice_file.dest_public_key)?;
    let dest_payment = invoice_file
        .dest_payment
        .parse()
        .map_err(|_| InvoiceFileError::ParseDestPaymentError)?;

    Ok(Invoice {
        invoice_id,
        dest_public_key,
        dest_payment,
    })
}

/// Store Invoice to file
pub fn store_invoice_to_file(invoice: &Invoice, path: &Path) -> Result<(), InvoiceFileError> {
    let Invoice {
        ref invoice_id,
        ref dest_public_key,
        dest_payment,
    } = invoice;

    let invoice_file = InvoiceFile {
        invoice_id: invoice_id_to_string(invoice_id),
        dest_public_key: public_key_to_string(dest_public_key),
        dest_payment: dest_payment.to_string(),
    };

    let data = toml::to_string(&invoice_file)?;

    let mut file = File::create(path)?;
    file.write_all(&data.as_bytes())?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    use app::invoice::{InvoiceId, INVOICE_ID_LEN};
    use app::PUBLIC_KEY_LEN;

    #[test]
    fn test_invoice_file_basic() {
        let invoice_file: InvoiceFile = toml::from_str(
            r#"
            invoice_id = 'invoice_id'
            dest_public_key = 'dest_public_key'
            dest_payment = '100'
        "#,
        )
        .unwrap();

        assert_eq!(invoice_file.invoice_id, "invoice_id");
        assert_eq!(invoice_file.dest_public_key, "dest_public_key");
        assert_eq!(invoice_file.dest_payment, "100");
    }

    #[test]
    fn test_store_load_invoice() {
        // Create a temporary directory:
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("invoice_file");

        let invoice = Invoice {
            invoice_id: InvoiceId::from(&[0; INVOICE_ID_LEN]),
            dest_public_key: PublicKey::from(&[1; PUBLIC_KEY_LEN]),
            dest_payment: 100,
        };

        store_invoice_to_file(&invoice, &file_path).unwrap();
        let invoice2 = load_invoice_from_file(&file_path).unwrap();

        assert_eq!(invoice, invoice2);
    }
}
