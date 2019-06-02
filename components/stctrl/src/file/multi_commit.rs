use std::fs::{self, File};
use std::io::{self, Write};
use std::path::Path;

use std::convert::TryFrom;

use derive_more::*;

use app::ser_string::{
    hash_result_to_string, hashed_lock_to_string, invoice_id_to_string, plain_lock_to_string,
    signature_to_string, string_to_hash_result, string_to_hashed_lock, string_to_invoice_id,
    string_to_plain_lock, string_to_signature, SerStringError,
};
use app::{Commit, MultiCommit};

use toml;

#[derive(Debug)]
pub struct ParseCommitFileError;

#[derive(Debug, From)]
pub enum MultiCommitFileError {
    IoError(io::Error),
    TomlDeError(toml::de::Error),
    TomlSeError(toml::ser::Error),
    SerStringError,
    ParseDestPaymentError,
    ParseTotalDestPaymentError,
    ParseCommitFileError,
    InvalidPublicKey,
}

impl From<SerStringError> for MultiCommitFileError {
    fn from(_e: SerStringError) -> Self {
        MultiCommitFileError::SerStringError
    }
}

impl From<ParseCommitFileError> for MultiCommitFileError {
    fn from(_e: ParseCommitFileError) -> Self {
        MultiCommitFileError::ParseCommitFileError
    }
}

/// Representing a Commit in an easy to serialize representation.
#[derive(Serialize, Deserialize)]
pub struct CommitFile {
    pub response_hash: String,
    pub dest_payment: String,
    pub src_plain_lock: String,
    pub dest_hashed_lock: String,
    pub signature: String,
}

impl std::convert::From<&Commit> for CommitFile {
    fn from(commit: &Commit) -> Self {
        CommitFile {
            response_hash: hash_result_to_string(&commit.response_hash),
            dest_payment: commit.dest_payment.to_string(),
            src_plain_lock: plain_lock_to_string(&commit.src_plain_lock),
            dest_hashed_lock: hashed_lock_to_string(&commit.dest_hashed_lock),
            signature: signature_to_string(&commit.signature),
        }
    }
}

impl std::convert::TryFrom<&CommitFile> for Commit {
    type Error = ParseCommitFileError;

    fn try_from(commit: &CommitFile) -> Result<Self, Self::Error> {
        Ok(Commit {
            response_hash: string_to_hash_result(&commit.response_hash)
                .map_err(|_| ParseCommitFileError)?,
            dest_payment: commit
                .dest_payment
                .parse()
                .map_err(|_| ParseCommitFileError)?,
            src_plain_lock: string_to_plain_lock(&commit.src_plain_lock)
                .map_err(|_| ParseCommitFileError)?,
            dest_hashed_lock: string_to_hashed_lock(&commit.dest_hashed_lock)
                .map_err(|_| ParseCommitFileError)?,
            signature: string_to_signature(&commit.signature).map_err(|_| ParseCommitFileError)?,
        })
    }
}

/// A helper structure for serialize and deserializing MultiCommit.
#[derive(Serialize, Deserialize)]
pub struct MultiCommitFile {
    pub invoice_id: String,
    pub total_dest_payment: String,
    pub commits: Vec<CommitFile>,
}

/// Load MultiCommit from a file
pub fn load_multi_commit_from_file(path: &Path) -> Result<MultiCommit, MultiCommitFileError> {
    let data = fs::read_to_string(&path)?;
    let multi_commit_file: MultiCommitFile = toml::from_str(&data)?;

    let invoice_id = string_to_invoice_id(&multi_commit_file.invoice_id)?;
    let total_dest_payment = multi_commit_file
        .total_dest_payment
        .parse()
        .map_err(|_| MultiCommitFileError::ParseTotalDestPaymentError)?;

    let mut commits: Vec<Commit> = Vec::new();
    for commit_file in &multi_commit_file.commits {
        commits.push(Commit::try_from(commit_file)?);
    }

    Ok(MultiCommit {
        invoice_id,
        total_dest_payment,
        commits,
    })
}

/// Store MultiCommit to file
pub fn store_multi_commit_to_file(
    multi_commit: &MultiCommit,
    path: &Path,
) -> Result<(), MultiCommitFileError> {
    let MultiCommit {
        ref invoice_id,
        total_dest_payment,
        ref commits,
    } = multi_commit;

    let multi_commit_file = MultiCommitFile {
        invoice_id: invoice_id_to_string(&invoice_id),
        total_dest_payment: total_dest_payment.to_string(),
        commits: commits.iter().map(|commit| commit.into()).collect(),
    };

    let data = toml::to_string(&multi_commit_file)?;

    let mut file = File::create(path)?;
    file.write_all(&data.as_bytes())?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    use app::invoice::{InvoiceId, INVOICE_ID_LEN};
    use app::{
        HashResult, HashedLock, PlainLock, Signature, HASHED_LOCK_LEN, HASH_RESULT_LEN,
        PLAIN_LOCK_LEN, SIGNATURE_LEN,
    };

    #[test]
    fn test_multi_commit_file_basic() {
        let multi_commit_file: MultiCommitFile = toml::from_str(
            r#"
            invoice_id = 'invoice_id'
            total_dest_payment = 'total_dest_payment'

            [[commits]]
            response_hash = 'response_hash0'
            dest_payment = 'dest_payment0'
            src_plain_lock = 'src_plain_lock0'
            dest_hashed_lock = 'dest_hashed_lock0'
            signature = 'signature0'

            [[commits]]
            response_hash = 'response_hash1'
            dest_payment = 'dest_payment1'
            src_plain_lock = 'src_plain_lock1'
            dest_hashed_lock = 'dest_hashed_lock1'
            signature = 'signature1'
        "#,
        )
        .unwrap();

        assert_eq!(multi_commit_file.invoice_id, "invoice_id");
        assert_eq!(multi_commit_file.total_dest_payment, "total_dest_payment");
        assert_eq!(multi_commit_file.commits[0].response_hash, "response_hash0");
        assert_eq!(multi_commit_file.commits[1].signature, "signature1");
    }

    #[test]
    fn test_store_load_multi_commit() {
        // Create a temporary directory:
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("multi_commit_file");

        let multi_commit = MultiCommit {
            invoice_id: InvoiceId::from(&[1; INVOICE_ID_LEN]),
            total_dest_payment: 200,
            commits: vec![
                Commit {
                    response_hash: HashResult::from(&[0u8; HASH_RESULT_LEN]),
                    dest_payment: 150,
                    src_plain_lock: PlainLock::from(&[1u8; PLAIN_LOCK_LEN]),
                    dest_hashed_lock: HashedLock::from(&[2u8; HASHED_LOCK_LEN]),
                    signature: Signature::from(&[3u8; SIGNATURE_LEN]),
                },
                Commit {
                    response_hash: HashResult::from(&[4u8; HASH_RESULT_LEN]),
                    dest_payment: 50,
                    src_plain_lock: PlainLock::from(&[5u8; PLAIN_LOCK_LEN]),
                    dest_hashed_lock: HashedLock::from(&[6u8; HASHED_LOCK_LEN]),
                    signature: Signature::from(&[7u8; SIGNATURE_LEN]),
                },
            ],
        };

        store_multi_commit_to_file(&multi_commit, &file_path).unwrap();
        let multi_commit2 = load_multi_commit_from_file(&file_path).unwrap();

        assert_eq!(multi_commit, multi_commit2);
    }
}
