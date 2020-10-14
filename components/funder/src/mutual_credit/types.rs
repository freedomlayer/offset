// use common::safe_arithmetic::SafeSignedArithmetic;

// Used for macros:
use paste::paste;

use common::async_rpc::OpError;
use common::ser_utils::ser_string;
use common::{get_out_type, ops_enum};

use proto::crypto::Uid;
use proto::funder::messages::PendingTransaction;

use futures::channel::{mpsc, oneshot};
use futures::SinkExt;

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
}

impl McBalance {
    // TODO: Remove unused hint
    #[allow(unused)]
    pub fn new(balance: i128) -> McBalance {
        McBalance {
            balance,
            local_pending_debt: 0,
            remote_pending_debt: 0,
        }
    }
}

ops_enum!((McOp, McTransaction) => {
    get_balance() -> McBalance;
    set_balance(balance: i128);
    set_local_pending_debt(debt: u128);
    set_remote_pending_debt(debt: u128);
    get_local_pending_transaction(request_id: Uid) -> Option<PendingTransaction>;
    insert_local_pending_transaction(pending_transaction: PendingTransaction);
    remove_local_pending_transaction(request_id: Uid);
    get_remote_pending_transaction(request_id: Uid) -> Option<PendingTransaction>;
    insert_remote_pending_transaction(pending_transaction: PendingTransaction);
    remove_remote_pending_transaction(request_id: Uid);
});
