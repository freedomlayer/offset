use std::fs::{self, File};
use std::io::{self, Write};
use std::path::Path;

use app::report::MoveTokenHashedReport;
use app::ser_string::{
    hash_result_to_string, public_key_to_string, rand_value_to_string, signature_to_string,
    string_to_hash_result, string_to_public_key, string_to_rand_value, string_to_signature,
    SerStringError,
};

use toml;

#[derive(Debug)]
pub enum MoveTokenHashedReportFileError {
    IoError(io::Error),
    TomlDeError(toml::de::Error),
    TomlSeError(toml::ser::Error),
    SerStringError,
    ParseInconsistencyCounterError,
    ParseMoveTokenCounterError,
    ParseBalanceError,
    ParseLocalPendingDebtError,
    ParseRemotePendingDebtError,
    InvalidPublicKey,
}

/// A helper structure for serialize and deserializing MoveTokenHashedReport.
#[derive(Serialize, Deserialize)]
pub struct MoveTokenHashedReportFile {
    pub prefix_hash: String,           // HashResult,
    pub local_public_key: String,      // PublicKey,
    pub remote_public_key: String,     // PublicKey,
    pub inconsistency_counter: String, // u64,
    pub move_token_counter: String,    // u128,
    pub balance: String,               // i128,
    pub local_pending_debt: String,    // u128,
    pub remote_pending_debt: String,   // u128,
    pub rand_nonce: String,            // RandValue,
    pub new_token: String,             // Signature,
}

impl From<io::Error> for MoveTokenHashedReportFileError {
    fn from(e: io::Error) -> Self {
        MoveTokenHashedReportFileError::IoError(e)
    }
}

impl From<toml::de::Error> for MoveTokenHashedReportFileError {
    fn from(e: toml::de::Error) -> Self {
        MoveTokenHashedReportFileError::TomlDeError(e)
    }
}

impl From<toml::ser::Error> for MoveTokenHashedReportFileError {
    fn from(e: toml::ser::Error) -> Self {
        MoveTokenHashedReportFileError::TomlSeError(e)
    }
}

impl From<SerStringError> for MoveTokenHashedReportFileError {
    fn from(_e: SerStringError) -> Self {
        MoveTokenHashedReportFileError::SerStringError
    }
}

/// Load MoveTokenHashedReport from a file
pub fn load_move_token_hashed_report_from_file(
    path: &Path,
) -> Result<MoveTokenHashedReport, MoveTokenHashedReportFileError> {
    let data = fs::read_to_string(&path)?;
    let move_token_hashed_report_file: MoveTokenHashedReportFile = toml::from_str(&data)?;

    Ok(MoveTokenHashedReport {
        prefix_hash: string_to_hash_result(&move_token_hashed_report_file.prefix_hash)?,
        local_public_key: string_to_public_key(&move_token_hashed_report_file.local_public_key)?,
        remote_public_key: string_to_public_key(&move_token_hashed_report_file.remote_public_key)?,
        inconsistency_counter: move_token_hashed_report_file
            .inconsistency_counter
            .parse()
            .map_err(|_| MoveTokenHashedReportFileError::ParseInconsistencyCounterError)?,
        move_token_counter: move_token_hashed_report_file
            .move_token_counter
            .parse()
            .map_err(|_| MoveTokenHashedReportFileError::ParseMoveTokenCounterError)?,
        balance: move_token_hashed_report_file
            .balance
            .parse()
            .map_err(|_| MoveTokenHashedReportFileError::ParseBalanceError)?,
        local_pending_debt: move_token_hashed_report_file
            .local_pending_debt
            .parse()
            .map_err(|_| MoveTokenHashedReportFileError::ParseLocalPendingDebtError)?,
        remote_pending_debt: move_token_hashed_report_file
            .remote_pending_debt
            .parse()
            .map_err(|_| MoveTokenHashedReportFileError::ParseRemotePendingDebtError)?,
        rand_nonce: string_to_rand_value(&move_token_hashed_report_file.rand_nonce)?,
        new_token: string_to_signature(&move_token_hashed_report_file.new_token)?,
    })
}

/// Store MoveTokenHashedReport to file
pub fn store_move_token_hashed_report_to_file(
    move_token_hashed_report: &MoveTokenHashedReport,
    path: &Path,
) -> Result<(), MoveTokenHashedReportFileError> {
    let MoveTokenHashedReport {
        ref prefix_hash,
        ref local_public_key,
        ref remote_public_key,
        inconsistency_counter,
        move_token_counter,
        balance,
        local_pending_debt,
        remote_pending_debt,
        ref rand_nonce,
        ref new_token,
    } = move_token_hashed_report;

    let move_token_hashed_report_file = MoveTokenHashedReportFile {
        prefix_hash: hash_result_to_string(prefix_hash),
        local_public_key: public_key_to_string(local_public_key),
        remote_public_key: public_key_to_string(remote_public_key),
        inconsistency_counter: inconsistency_counter.to_string(),
        move_token_counter: move_token_counter.to_string(),
        balance: balance.to_string(),
        local_pending_debt: local_pending_debt.to_string(),
        remote_pending_debt: remote_pending_debt.to_string(),
        rand_nonce: rand_value_to_string(rand_nonce),
        new_token: signature_to_string(new_token),
    };

    let data = toml::to_string(&move_token_hashed_report_file)?;

    let mut file = File::create(path)?;
    file.write(&data.as_bytes())?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    use app::{
        HashResult, PublicKey, RandValue, Signature, HASH_RESULT_LEN, PUBLIC_KEY_LEN,
        RAND_VALUE_LEN, SIGNATURE_LEN,
    };

    #[test]
    fn test_move_token_hashed_report_file_basic() {
        let move_token_hashed_report_file: MoveTokenHashedReportFile = toml::from_str(
            r#"
            prefix_hash = 'prefix_hash'
            local_public_key = 'local_public_key'
            remote_public_key = 'remote_public_key'
            inconsistency_counter = 'inconsistency_counter'
            move_token_counter = 'move_token_counter'
            balance = 'balance'
            local_pending_debt = 'local_pending_debt'
            remote_pending_debt = 'remote_pending_debt'
            rand_nonce = 'rand_nonce'
            new_token = 'new_token'
        "#,
        )
        .unwrap();

        assert_eq!(move_token_hashed_report_file.prefix_hash, "prefix_hash");
        assert_eq!(
            move_token_hashed_report_file.local_public_key,
            "local_public_key"
        );
        assert_eq!(
            move_token_hashed_report_file.remote_public_key,
            "remote_public_key"
        );
        assert_eq!(
            move_token_hashed_report_file.inconsistency_counter,
            "inconsistency_counter"
        );
        assert_eq!(
            move_token_hashed_report_file.move_token_counter,
            "move_token_counter"
        );
        assert_eq!(move_token_hashed_report_file.balance, "balance");
        assert_eq!(
            move_token_hashed_report_file.local_pending_debt,
            "local_pending_debt"
        );
        assert_eq!(
            move_token_hashed_report_file.remote_pending_debt,
            "remote_pending_debt"
        );
        assert_eq!(move_token_hashed_report_file.rand_nonce, "rand_nonce");
        assert_eq!(move_token_hashed_report_file.new_token, "new_token");
    }

    #[test]
    fn test_store_load_move_token_hashed_report() {
        // Create a temporary directory:
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("move_token_hashed_report_file");

        let move_token_hashed_report = MoveTokenHashedReport {
            prefix_hash: HashResult::from(&[0; HASH_RESULT_LEN]),
            local_public_key: PublicKey::from(&[1; PUBLIC_KEY_LEN]),
            remote_public_key: PublicKey::from(&[2; PUBLIC_KEY_LEN]),
            inconsistency_counter: 8u64,
            move_token_counter: 15u128,
            balance: -1500i128,
            local_pending_debt: 10u128,
            remote_pending_debt: 20u128,
            rand_nonce: RandValue::from(&[3; RAND_VALUE_LEN]),
            new_token: Signature::from(&[4; SIGNATURE_LEN]),
        };

        store_move_token_hashed_report_to_file(&move_token_hashed_report, &file_path).unwrap();
        let move_token_hashed_report2 =
            load_move_token_hashed_report_from_file(&file_path).unwrap();

        assert_eq!(move_token_hashed_report, move_token_hashed_report2);
    }
}
