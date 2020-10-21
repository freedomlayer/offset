// use common::safe_arithmetic::SafeSignedArithmetic;

use common::async_rpc::AsyncOpResult;
use common::ser_utils::ser_string;

use proto::crypto::Uid;
use proto::funder::messages::PendingTransaction;

#[derive(Arbitrary, Clone, Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct McBalance {
    /// Amount of credits this side has against the remote side.
    /// The other side keeps the negation of this value.
    #[serde(with = "ser_string")]
    pub balance: i128,
    /// Frozen credits by our side
    #[serde(with = "ser_string")]
    pub local_pending_debt: u128,
    /// Frozen credits by the remote side
    #[serde(with = "ser_string")]
    pub remote_pending_debt: u128,
    // TODO: in_fees and out_fees should be u256, not u128!
    /// Fees that were received from remote side
    #[serde(with = "ser_string")]
    pub in_fees: u128,
    /// Fees that were given to remote side
    #[serde(with = "ser_string")]
    pub out_fees: u128,
}

impl McBalance {
    // TODO: Remove unused hint
    #[allow(unused)]
    pub fn new(balance: i128) -> McBalance {
        McBalance {
            balance,
            local_pending_debt: 0,
            remote_pending_debt: 0,
            in_fees: 0,
            out_fees: 0,
        }
    }
}

pub trait McTransaction {
    fn get_balance(&mut self) -> AsyncOpResult<McBalance>;
    fn set_balance(&mut self, new_balance: i128) -> AsyncOpResult<()>;
    fn set_local_pending_debt(&mut self, debt: u128) -> AsyncOpResult<()>;
    fn set_remote_pending_debt(&mut self, debt: u128) -> AsyncOpResult<()>;
    fn set_in_fees(&mut self, in_fees: u128) -> AsyncOpResult<()>;
    fn set_out_fees(&mut self, out_fees: u128) -> AsyncOpResult<()>;
    fn get_local_pending_transaction(
        &mut self,
        request_id: Uid,
    ) -> AsyncOpResult<Option<PendingTransaction>>;
    fn insert_local_pending_transaction(
        &mut self,
        pending_transaction: PendingTransaction,
    ) -> AsyncOpResult<()>;
    fn remove_local_pending_transaction(&mut self, request_id: Uid) -> AsyncOpResult<()>;
    fn get_remote_pending_transaction(
        &mut self,
        request_id: Uid,
    ) -> AsyncOpResult<Option<PendingTransaction>>;
    fn insert_remote_pending_transaction(
        &mut self,
        pending_transaction: PendingTransaction,
    ) -> AsyncOpResult<()>;
    fn remove_remote_pending_transaction(&mut self, request_id: Uid) -> AsyncOpResult<()>;
}
