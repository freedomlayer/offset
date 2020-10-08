use std::collections::HashMap as ImHashMap;

use common::safe_arithmetic::SafeSignedArithmetic;
use common::ser_utils::{ser_b64, ser_map_b64_any, ser_string};

use proto::crypto::{PublicKey, Uid};
use proto::funder::messages::{Currency, PendingTransaction};

use futures::channel::{mpsc, oneshot};
use futures::SinkExt;

/*
// TODO: Where do we need to check this value?
/// The maximum possible funder debt.
/// We don't use the full u128 because i128 can not go beyond this value.
pub const MAX_FUNDER_DEBT: u128 = (1 << 127) - 1;
*/

// TODO: Rename this to McIdents
#[derive(Arbitrary, Clone, Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct McIdents {
    /// My public key
    #[serde(with = "ser_b64")]
    pub local_public_key: PublicKey,
    /// Friend's public key
    #[serde(with = "ser_b64")]
    pub remote_public_key: PublicKey,
}

// TODO: Rename this to McBalance
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
    fn new(balance: i128) -> McBalance {
        McBalance {
            balance,
            local_pending_debt: 0,
            remote_pending_debt: 0,
        }
    }
}

#[derive(Arbitrary, Clone, Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct McPendingTransactions {
    /// Pending transactions that were opened locally and not yet completed
    #[serde(with = "ser_map_b64_any")]
    pub local: ImHashMap<Uid, PendingTransaction>,
    /// Pending transactions that were opened remotely and not yet completed
    #[serde(with = "ser_map_b64_any")]
    pub remote: ImHashMap<Uid, PendingTransaction>,
}

impl McPendingTransactions {
    fn new() -> McPendingTransactions {
        McPendingTransactions {
            local: ImHashMap::new(),
            remote: ImHashMap::new(),
        }
    }
}

#[derive(Arbitrary, Clone, Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct MutualCreditState {
    /// Public identities of local and remote side
    pub idents: McIdents,
    /// Currency in use (How much is one credit worth?)
    pub currency: Currency,
    /// Current credit balance with respect to remote side
    pub balance: McBalance,
    /// Requests in progress
    pub pending_transactions: McPendingTransactions,
}

#[derive(Arbitrary, Clone, Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct MutualCredit {
    state: MutualCreditState,
}

#[derive(Arbitrary, Eq, PartialEq, Debug, Clone)]
pub enum McMutationOld {
    SetBalance(i128),
    InsertLocalPendingTransaction(PendingTransaction),
    RemoveLocalPendingTransaction(Uid),
    InsertRemotePendingTransaction(PendingTransaction),
    RemoveRemotePendingTransaction(Uid),
    SetLocalPendingDebt(u128),
    SetRemotePendingDebt(u128),
}

#[derive(Debug)]
pub enum McOpError {
    SendOpFailed,
    ResponseOpFailed(oneshot::Canceled),
}

pub type McOpResult<T> = Result<T, McOpError>;
pub type McOpSenderResult<T> = oneshot::Sender<McOpResult<T>>;

#[derive(Debug)]
pub enum McOp {
    GetBalance(McOpSenderResult<McBalance>),
    SetBalance(i128, McOpSenderResult<()>),
    SetLocalPendingDebt(u128, McOpSenderResult<()>),
    SetRemotePendingDebt(u128, McOpSenderResult<()>),
    GetLocalPendingTransaction(Uid, McOpSenderResult<Option<PendingTransaction>>),
    InsertLocalPendingTransaction(PendingTransaction, McOpSenderResult<()>),
    RemoveLocalPendingTransaction(Uid, McOpSenderResult<()>),
    GetRemotePendingTransaction(Uid, McOpSenderResult<Option<PendingTransaction>>),
    InsertRemotePendingTransaction(PendingTransaction, McOpSenderResult<()>),
    RemoveRemotePendingTransaction(Uid, McOpSenderResult<()>),
    // Commit(McOpSenderResult<()>),
}

// TODO: Remove:
#[allow(unused)]
struct McTransaction {
    sender: mpsc::Sender<McOp>,
}

// TODO: Remove:
#[allow(unused)]
impl McTransaction {
    async fn get_balance(&mut self) -> McOpResult<McBalance> {
        let (op_sender, op_receiver) = oneshot::channel();
        let op = McOp::GetBalance(op_sender);
        self.sender
            .send(op)
            .await
            .map_err(|_| McOpError::SendOpFailed)?;
        op_receiver.await.map_err(McOpError::ResponseOpFailed)?
    }
    async fn set_balance(&mut self, balance: i128) -> McOpResult<()> {
        let (op_sender, op_receiver) = oneshot::channel();
        let op = McOp::SetBalance(balance, op_sender);
        self.sender
            .send(op)
            .await
            .map_err(|_| McOpError::SendOpFailed)?;
        op_receiver.await.map_err(McOpError::ResponseOpFailed)?
    }
    async fn set_local_pending_debt(&mut self, local_pending_debt: u128) -> McOpResult<()> {
        let (op_sender, op_receiver) = oneshot::channel();
        let op = McOp::SetLocalPendingDebt(local_pending_debt, op_sender);
        self.sender
            .send(op)
            .await
            .map_err(|_| McOpError::SendOpFailed)?;
        op_receiver.await.map_err(McOpError::ResponseOpFailed)?
    }
    async fn set_remote_pending_debt(&mut self, remote_pending_debt: u128) -> McOpResult<()> {
        let (op_sender, op_receiver) = oneshot::channel();
        let op = McOp::SetRemotePendingDebt(remote_pending_debt, op_sender);
        self.sender
            .send(op)
            .await
            .map_err(|_| McOpError::SendOpFailed)?;
        op_receiver.await.map_err(McOpError::ResponseOpFailed)?
    }
    async fn get_local_pending_transaction(
        &mut self,
        request_id: Uid,
    ) -> McOpResult<Option<PendingTransaction>> {
        let (op_sender, op_receiver) = oneshot::channel();
        let op = McOp::GetLocalPendingTransaction(request_id, op_sender);
        self.sender
            .send(op)
            .await
            .map_err(|_| McOpError::SendOpFailed)?;
        op_receiver.await.map_err(McOpError::ResponseOpFailed)?
    }
    async fn insert_local_pending_transaction(
        &mut self,
        pending_transaction: PendingTransaction,
    ) -> McOpResult<()> {
        let (op_sender, op_receiver) = oneshot::channel();
        let op = McOp::InsertLocalPendingTransaction(pending_transaction, op_sender);
        self.sender
            .send(op)
            .await
            .map_err(|_| McOpError::SendOpFailed)?;
        op_receiver.await.map_err(McOpError::ResponseOpFailed)?
    }
    async fn remove_local_pending_transaction(&mut self, request_id: Uid) -> McOpResult<()> {
        let (op_sender, op_receiver) = oneshot::channel();
        let op = McOp::RemoveLocalPendingTransaction(request_id, op_sender);
        self.sender
            .send(op)
            .await
            .map_err(|_| McOpError::SendOpFailed)?;
        op_receiver.await.map_err(McOpError::ResponseOpFailed)?
    }
    async fn get_remote_pending_transaction(
        &mut self,
        request_id: Uid,
    ) -> McOpResult<Option<PendingTransaction>> {
        let (op_sender, op_receiver) = oneshot::channel();
        let op = McOp::GetRemotePendingTransaction(request_id, op_sender);
        self.sender
            .send(op)
            .await
            .map_err(|_| McOpError::SendOpFailed)?;
        op_receiver.await.map_err(McOpError::ResponseOpFailed)?
    }
    async fn insert_remote_pending_transaction(
        &mut self,
        pending_transaction: PendingTransaction,
    ) -> McOpResult<()> {
        let (op_sender, op_receiver) = oneshot::channel();
        let op = McOp::InsertRemotePendingTransaction(pending_transaction, op_sender);
        self.sender
            .send(op)
            .await
            .map_err(|_| McOpError::SendOpFailed)?;
        op_receiver.await.map_err(McOpError::ResponseOpFailed)?
    }
    async fn remove_remote_pending_transaction(&mut self, request_id: Uid) -> McOpResult<()> {
        let (op_sender, op_receiver) = oneshot::channel();
        let op = McOp::RemoveRemotePendingTransaction(request_id, op_sender);
        self.sender
            .send(op)
            .await
            .map_err(|_| McOpError::SendOpFailed)?;
        op_receiver.await.map_err(McOpError::ResponseOpFailed)?
    }

    /*
    async fn commit(mut self) -> McOpResult<()> {
        let (op_sender, op_receiver) = oneshot::channel();
        let op = McOp::Commit(op_sender);
        self.sender
            .send(op)
            .await
            .map_err(|_| McOpError::SendOpFailed)?;
        op_receiver.await.map_err(McOpError::ResponseOpFailed)?
    }
    */
}

impl MutualCredit {
    pub fn new(
        // TODO: Should we move instead of take a reference here?
        local_public_key: &PublicKey,
        remote_public_key: &PublicKey,
        currency: &Currency,
        balance: i128,
    ) -> MutualCredit {
        MutualCredit {
            state: MutualCreditState {
                idents: McIdents {
                    local_public_key: local_public_key.clone(),
                    remote_public_key: remote_public_key.clone(),
                },
                currency: currency.clone(),
                balance: McBalance::new(balance),
                pending_transactions: McPendingTransactions::new(),
            },
        }
    }

    // TODO: Remove unused hint
    #[allow(unused)]
    /// Calculate required balance for reset.
    /// This would be current balance plus additional future profits.
    pub fn balance_for_reset(&self) -> i128 {
        self.state
            .balance
            .balance
            .checked_add_unsigned(self.state.balance.remote_pending_debt)
            .expect("Overflow when calculating balance_for_reset")
        // TODO: Is this the correct formula?
        // Other options:
        // *    balance
        // *    balance + remote_pending_debt - local_pending_debt
    }

    pub fn state(&self) -> &MutualCreditState {
        &self.state
    }

    pub fn mutate(&mut self, mc_mutation: &McMutationOld) {
        match mc_mutation {
            McMutationOld::SetBalance(balance) => self.set_balance(*balance),
            McMutationOld::InsertLocalPendingTransaction(pending_friend_request) => {
                self.insert_local_pending_transaction(pending_friend_request)
            }
            McMutationOld::RemoveLocalPendingTransaction(request_id) => {
                self.remove_local_pending_transaction(request_id)
            }
            McMutationOld::InsertRemotePendingTransaction(pending_friend_request) => {
                self.insert_remote_pending_transaction(pending_friend_request)
            }
            McMutationOld::RemoveRemotePendingTransaction(request_id) => {
                self.remove_remote_pending_transaction(request_id)
            }
            McMutationOld::SetLocalPendingDebt(local_pending_debt) => {
                self.set_local_pending_debt(*local_pending_debt)
            }
            McMutationOld::SetRemotePendingDebt(remote_pending_debt) => {
                self.set_remote_pending_debt(*remote_pending_debt)
            }
        }
    }

    fn set_balance(&mut self, balance: i128) {
        self.state.balance.balance = balance;
    }

    fn insert_remote_pending_transaction(&mut self, pending_friend_request: &PendingTransaction) {
        self.state.pending_transactions.remote.insert(
            pending_friend_request.request_id.clone(),
            pending_friend_request.clone(),
        );
    }

    fn remove_remote_pending_transaction(&mut self, request_id: &Uid) {
        let _ = self.state.pending_transactions.remote.remove(request_id);
    }

    fn insert_local_pending_transaction(&mut self, pending_friend_request: &PendingTransaction) {
        self.state.pending_transactions.local.insert(
            pending_friend_request.request_id.clone(),
            pending_friend_request.clone(),
        );
    }

    fn remove_local_pending_transaction(&mut self, request_id: &Uid) {
        let _ = self.state.pending_transactions.local.remove(request_id);
    }

    fn set_remote_pending_debt(&mut self, remote_pending_debt: u128) {
        self.state.balance.remote_pending_debt = remote_pending_debt;
    }

    fn set_local_pending_debt(&mut self, local_pending_debt: u128) {
        self.state.balance.local_pending_debt = local_pending_debt;
    }
}
