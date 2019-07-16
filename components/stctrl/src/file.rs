use app::ser_string::{from_base64, from_string, to_base64, to_string};

use app::crypto::{
    HashResult, HashedLock, InvoiceId, PaymentId, PlainLock, PublicKey, RandValue, Signature,
};
use app::report::MoveTokenHashedReport;
use app::{Commit, MultiCommit, Receipt};

use mutual_from::mutual_from;

#[derive(Serialize, Deserialize)]
pub struct InvoiceFile {
    #[serde(serialize_with = "to_base64", deserialize_with = "from_base64")]
    pub invoice_id: InvoiceId,
    #[serde(serialize_with = "to_base64", deserialize_with = "from_base64")]
    pub dest_public_key: PublicKey,
    #[serde(serialize_with = "to_string", deserialize_with = "from_string")]
    pub dest_payment: u128,
}

/// Representing a Commit in an easy to serialize representation.
#[mutual_from(Commit)]
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

/// A helper structure for serialize and deserializing MultiCommit.
#[derive(Serialize, Deserialize)]
pub struct MultiCommitFile {
    #[serde(serialize_with = "to_base64", deserialize_with = "from_base64")]
    pub invoice_id: InvoiceId,
    #[serde(serialize_with = "to_string", deserialize_with = "from_string")]
    pub total_dest_payment: u128,
    pub commits: Vec<CommitFile>,
}

/// A helper structure for serialize and deserializing Payment.
#[derive(Serialize, Deserialize)]
pub struct PaymentFile {
    #[serde(serialize_with = "to_base64", deserialize_with = "from_base64")]
    pub payment_id: PaymentId,
}

/// A helper structure for serialize and deserializing Receipt.
#[mutual_from(Receipt)]
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

/// A helper structure for serialize and deserializing Token.
#[mutual_from(MoveTokenHashedReport)]
#[derive(Serialize, Deserialize)]
pub struct TokenFile {
    #[serde(serialize_with = "to_base64", deserialize_with = "from_base64")]
    pub prefix_hash: HashResult,
    #[serde(serialize_with = "to_base64", deserialize_with = "from_base64")]
    pub local_public_key: PublicKey,
    #[serde(serialize_with = "to_base64", deserialize_with = "from_base64")]
    pub remote_public_key: PublicKey,
    pub inconsistency_counter: u64,
    #[serde(serialize_with = "to_string", deserialize_with = "from_string")]
    pub move_token_counter: u128,
    #[serde(serialize_with = "to_string", deserialize_with = "from_string")]
    pub balance: i128,
    #[serde(serialize_with = "to_string", deserialize_with = "from_string")]
    pub local_pending_debt: u128,
    #[serde(serialize_with = "to_string", deserialize_with = "from_string")]
    pub remote_pending_debt: u128,
    #[serde(serialize_with = "to_base64", deserialize_with = "from_base64")]
    pub rand_nonce: RandValue,
    #[serde(serialize_with = "to_base64", deserialize_with = "from_base64")]
    pub new_token: Signature,
}

impl std::convert::From<MultiCommit> for MultiCommitFile {
    fn from(input: MultiCommit) -> Self {
        MultiCommitFile {
            invoice_id: input.invoice_id,
            total_dest_payment: input.total_dest_payment,
            commits: input.commits.into_iter().map(CommitFile::from).collect(),
        }
    }
}

impl std::convert::From<MultiCommitFile> for MultiCommit {
    fn from(input: MultiCommitFile) -> Self {
        MultiCommit {
            invoice_id: input.invoice_id,
            total_dest_payment: input.total_dest_payment,
            commits: input.commits.into_iter().map(Commit::from).collect(),
        }
    }
}
