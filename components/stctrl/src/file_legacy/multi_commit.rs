use std::fs::{self, File};
use std::io::{self, Write};
use std::path::Path;

use std::convert::TryFrom;

use derive_more::*;

use app::invoice::InvoiceId;
use app::ser_string::{from_base64, from_string, to_base64, to_string, SerStringError};
use app::{Commit, HashResult, HashedLock, MultiCommit, PlainLock, Signature};

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
    #[serde(serialize_with = "to_base64", deserialize_with = "from_base64")]
    pub response_hash: HashResult,
    #[serde(serialize_with = "to_string", deserialize_with = "from_string")]
    pub dest_payment: u128,
    #[serde(serialize_with = "to_base64", deserialize_with = "from_base64")]
    pub src_plain_lock: PlainLock,
    #[serde(serialize_with = "to_base64", deserialize_with = "from_base64")]
    pub dest_hashed_lock: HashedLock,
    #[serde(serialize_with = "to_base64", deserialize_with = "from_base64")]
    pub signature: Signature,
}

impl std::convert::From<&Commit> for CommitFile {
    fn from(commit: &Commit) -> Self {
        CommitFile {
            response_hash: commit.response_hash.clone(),
            dest_payment: commit.dest_payment,
            src_plain_lock: commit.src_plain_lock.clone(),
            dest_hashed_lock: commit.dest_hashed_lock.clone(),
            signature: commit.signature.clone(),
        }
    }
}

impl std::convert::TryFrom<&CommitFile> for Commit {
    type Error = ParseCommitFileError;

    fn try_from(commit: &CommitFile) -> Result<Self, Self::Error> {
        Ok(Commit {
            response_hash: commit.response_hash.clone(),
            dest_payment: commit.dest_payment,
            src_plain_lock: commit.src_plain_lock.clone(),
            dest_hashed_lock: commit.dest_hashed_lock.clone(),
            signature: commit.signature.clone(),
        })
    }
}

/// A helper structure for serialize and deserializing MultiCommit.
#[derive(Serialize, Deserialize)]
pub struct MultiCommitFile {
    #[serde(serialize_with = "to_base64", deserialize_with = "from_base64")]
    pub invoice_id: InvoiceId,
    #[serde(serialize_with = "to_string", deserialize_with = "from_string")]
    pub total_dest_payment: u128,
    pub commits: Vec<CommitFile>,
}

/// Load MultiCommit from a file
pub fn load_multi_commit_from_file(path: &Path) -> Result<MultiCommit, MultiCommitFileError> {
    let data = fs::read_to_string(&path)?;
    let multi_commit_file: MultiCommitFile = toml::from_str(&data)?;

    let mut commits: Vec<Commit> = Vec::new();
    for commit_file in &multi_commit_file.commits {
        commits.push(Commit::try_from(commit_file)?);
    }

    Ok(MultiCommit {
        invoice_id: multi_commit_file.invoice_id,
        total_dest_payment: multi_commit_file.total_dest_payment,
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
        invoice_id: invoice_id.clone(),
        total_dest_payment: *total_dest_payment,
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

    /*
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
    */

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
