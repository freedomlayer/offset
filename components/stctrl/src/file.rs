use app::ser_utils::{SerBase64, SerString};

use app::common::{
    Commit, Currency, HashResult, HashedLock, InvoiceId, PaymentId, PlainLock, PublicKey,
    RandValue, Receipt, Signature,
};
use app::report::{MoveTokenHashedReport, TokenInfo};

use mutual_from::mutual_from;

#[derive(Arbitrary, Serialize, Deserialize, Debug, Clone)]
pub struct InvoiceFile {
    #[serde(with = "SerBase64")]
    pub invoice_id: InvoiceId,
    #[serde(with = "SerString")]
    pub currency: Currency,
    #[serde(with = "SerBase64")]
    pub dest_public_key: PublicKey,
    #[serde(with = "SerString")]
    pub dest_payment: u128,
}

/// Representing a Commit in an easy to serialize representation.
#[mutual_from(Commit)]
#[derive(Arbitrary, Clone, Serialize, Deserialize, Debug)]
pub struct CommitFile {
    #[serde(with = "SerBase64")]
    pub response_hash: HashResult,
    #[serde(with = "SerBase64")]
    pub src_plain_lock: PlainLock,
    #[serde(with = "SerBase64")]
    pub dest_hashed_lock: HashedLock,
    #[serde(with = "SerString")]
    pub dest_payment: u128,
    #[serde(with = "SerString")]
    pub total_dest_payment: u128,
    #[serde(with = "SerBase64")]
    pub invoice_id: InvoiceId,
    #[serde(with = "SerString")]
    pub currency: Currency,
    #[serde(with = "SerBase64")]
    pub signature: Signature,
}

/// A helper structure for serialize and deserializing Payment.
#[derive(Arbitrary, Clone, Serialize, Deserialize, Debug)]
pub struct PaymentFile {
    #[serde(with = "SerBase64")]
    pub payment_id: PaymentId,
}

/// A helper structure for serialize and deserializing Receipt.
#[mutual_from(Receipt)]
#[derive(Arbitrary, Clone, Serialize, Deserialize, Debug)]
pub struct ReceiptFile {
    #[serde(with = "SerBase64")]
    pub response_hash: HashResult,
    #[serde(with = "SerBase64")]
    pub invoice_id: InvoiceId,
    #[serde(with = "SerString")]
    pub currency: Currency,
    #[serde(with = "SerBase64")]
    pub src_plain_lock: PlainLock,
    #[serde(with = "SerBase64")]
    pub dest_plain_lock: PlainLock,
    pub is_complete: bool,
    #[serde(with = "SerString")]
    pub dest_payment: u128,
    #[serde(with = "SerString")]
    pub total_dest_payment: u128,
    #[serde(with = "SerBase64")]
    pub signature: Signature,
}

/// A helper structure for serialize and deserializing Token.
#[mutual_from(MoveTokenHashedReport)]
#[derive(Arbitrary, Clone, Serialize, Deserialize, Debug)]
pub struct TokenFile {
    #[serde(with = "SerBase64")]
    pub prefix_hash: HashResult,
    #[serde(with = "SerBase64")]
    pub rand_nonce: RandValue,
    #[serde(with = "SerBase64")]
    pub new_token: Signature,
    pub token_info: TokenInfo,
}

#[cfg(test)]
mod test {
    use super::*;

    use std::convert::TryFrom;

    use app::report::{BalanceInfo, CountersInfo, CurrencyBalanceInfo, McInfo};

    #[test]
    fn test_serialize_invoice_file() {
        let invoice_file = InvoiceFile {
            invoice_id: InvoiceId::from(&[1u8; InvoiceId::len()]),
            currency: Currency::try_from("FST".to_owned()).unwrap(),
            dest_public_key: PublicKey::from(&[0xbb; PublicKey::len()]),
            dest_payment: 10u128,
        };

        let _ = toml::to_string(&invoice_file).unwrap();
    }

    #[test]
    fn test_serialize_multi_commit_file() {
        let commit_file = CommitFile {
            response_hash: HashResult::from(&[0u8; HashResult::len()]),
            src_plain_lock: PlainLock::from(&[1u8; PlainLock::len()]),
            dest_hashed_lock: HashedLock::from(&[3u8; HashedLock::len()]),
            dest_payment: 4u128,
            total_dest_payment: 5u128,
            invoice_id: InvoiceId::from(&[6u8; InvoiceId::len()]),
            currency: Currency::try_from("FST".to_owned()).unwrap(),
            signature: Signature::from(&[7u8; Signature::len()]),
        };

        let _ = toml::to_string(&commit_file).unwrap();
    }

    /// Check if we can serialize TokenFile into TOML without crasing
    #[test]
    fn test_serialize_token_file() {
        let token_info = TokenInfo {
            mc: McInfo {
                local_public_key: PublicKey::from(&[1; PublicKey::len()]),
                remote_public_key: PublicKey::from(&[2; PublicKey::len()]),
                balances: vec![CurrencyBalanceInfo {
                    currency: "FST".parse().unwrap(),
                    balance_info: BalanceInfo {
                        balance: -5i128,
                        local_pending_debt: 16u128,
                        remote_pending_debt: 32u128,
                    },
                }],
            },
            counters: CountersInfo {
                inconsistency_counter: 3u64,
                move_token_counter: 4u128,
            },
        };

        let token_file = TokenFile {
            prefix_hash: HashResult::from(&[0; HashResult::len()]),
            rand_nonce: RandValue::from(&[1; RandValue::len()]),
            new_token: Signature::from(&[2; Signature::len()]),
            token_info,
        };

        let _ = toml::to_string(&token_file).unwrap();
    }
}
