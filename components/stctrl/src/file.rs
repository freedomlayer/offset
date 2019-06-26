use app::ser_string::{from_base64, from_string, to_base64, to_string};

use app::invoice::InvoiceId;
use app::{
    Commit, HashResult, HashedLock, MultiCommit, PlainLock, PublicKey, RandValue, Receipt,
    Signature,
};

use app::payment::PaymentId;
use app::report::MoveTokenHashedReport;

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

// TODO: Create a macro that does the following ergonomic conversion automatically:

impl std::convert::From<Commit> for CommitFile {
    fn from(input: Commit) -> Self {
        CommitFile {
            response_hash: input.response_hash.clone(),
            dest_payment: input.dest_payment,
            src_plain_lock: input.src_plain_lock.clone(),
            dest_hashed_lock: input.dest_hashed_lock.clone(),
            signature: input.signature.clone(),
        }
    }
}

impl std::convert::From<CommitFile> for Commit {
    fn from(input: CommitFile) -> Self {
        Commit {
            response_hash: input.response_hash.clone(),
            dest_payment: input.dest_payment,
            src_plain_lock: input.src_plain_lock.clone(),
            dest_hashed_lock: input.dest_hashed_lock.clone(),
            signature: input.signature.clone(),
        }
    }
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

impl std::convert::From<Receipt> for ReceiptFile {
    fn from(input: Receipt) -> Self {
        ReceiptFile {
            response_hash: input.response_hash,
            invoice_id: input.invoice_id,
            src_plain_lock: input.src_plain_lock,
            dest_plain_lock: input.dest_plain_lock,
            dest_payment: input.dest_payment,
            total_dest_payment: input.total_dest_payment,
            signature: input.signature,
        }
    }
}

impl std::convert::From<ReceiptFile> for Receipt {
    fn from(input: ReceiptFile) -> Self {
        Receipt {
            response_hash: input.response_hash,
            invoice_id: input.invoice_id,
            src_plain_lock: input.src_plain_lock,
            dest_plain_lock: input.dest_plain_lock,
            dest_payment: input.dest_payment,
            total_dest_payment: input.total_dest_payment,
            signature: input.signature,
        }
    }
}

impl std::convert::From<MoveTokenHashedReport> for TokenFile {
    fn from(input: MoveTokenHashedReport) -> Self {
        TokenFile {
            prefix_hash: input.prefix_hash,
            local_public_key: input.local_public_key,
            remote_public_key: input.remote_public_key,
            inconsistency_counter: input.inconsistency_counter,
            move_token_counter: input.move_token_counter,
            balance: input.balance,
            local_pending_debt: input.local_pending_debt,
            remote_pending_debt: input.remote_pending_debt,
            rand_nonce: input.rand_nonce,
            new_token: input.new_token,
        }
    }
}

impl std::convert::From<TokenFile> for MoveTokenHashedReport {
    fn from(input: TokenFile) -> Self {
        MoveTokenHashedReport {
            prefix_hash: input.prefix_hash,
            local_public_key: input.local_public_key,
            remote_public_key: input.remote_public_key,
            inconsistency_counter: input.inconsistency_counter,
            move_token_counter: input.move_token_counter,
            balance: input.balance,
            local_pending_debt: input.local_pending_debt,
            remote_pending_debt: input.remote_pending_debt,
            rand_nonce: input.rand_nonce,
            new_token: input.new_token,
        }
    }
}
