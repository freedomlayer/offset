// use common::safe_arithmetic::SafeSignedArithmetic;
use common::ser_utils::ser_string;

use proto::crypto::Uid;
use proto::funder::messages::PendingTransaction;

use futures::channel::{mpsc, oneshot};
use futures::SinkExt;

#[derive(Debug)]
pub enum McOpError {
    SendOpFailed,
    ResponseOpFailed(oneshot::Canceled),
}

pub type McOpResult<T> = Result<T, McOpError>;
pub type McOpSenderResult<T> = oneshot::Sender<McOpResult<T>>;

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

// TODO: Remove unused hint:
#[allow(unused)]
#[derive(Debug)]
pub struct McTransaction {
    sender: mpsc::Sender<McOp>,
}

// TODO: Remove unused hint:
#[allow(unused)]
impl McTransaction {
    pub fn new(sender: mpsc::Sender<McOp>) -> Self {
        Self { sender }
    }
    pub async fn get_balance(&mut self) -> McOpResult<McBalance> {
        let (op_sender, op_receiver) = oneshot::channel();
        let op = McOp::GetBalance(op_sender);
        self.sender
            .send(op)
            .await
            .map_err(|_| McOpError::SendOpFailed)?;
        op_receiver.await.map_err(McOpError::ResponseOpFailed)?
    }
    pub async fn set_balance(&mut self, balance: i128) -> McOpResult<()> {
        let (op_sender, op_receiver) = oneshot::channel();
        let op = McOp::SetBalance(balance, op_sender);
        self.sender
            .send(op)
            .await
            .map_err(|_| McOpError::SendOpFailed)?;
        op_receiver.await.map_err(McOpError::ResponseOpFailed)?
    }
    pub async fn set_local_pending_debt(&mut self, local_pending_debt: u128) -> McOpResult<()> {
        let (op_sender, op_receiver) = oneshot::channel();
        let op = McOp::SetLocalPendingDebt(local_pending_debt, op_sender);
        self.sender
            .send(op)
            .await
            .map_err(|_| McOpError::SendOpFailed)?;
        op_receiver.await.map_err(McOpError::ResponseOpFailed)?
    }
    pub async fn set_remote_pending_debt(&mut self, remote_pending_debt: u128) -> McOpResult<()> {
        let (op_sender, op_receiver) = oneshot::channel();
        let op = McOp::SetRemotePendingDebt(remote_pending_debt, op_sender);
        self.sender
            .send(op)
            .await
            .map_err(|_| McOpError::SendOpFailed)?;
        op_receiver.await.map_err(McOpError::ResponseOpFailed)?
    }
    pub async fn get_local_pending_transaction(
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
    pub async fn insert_local_pending_transaction(
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
    pub async fn remove_local_pending_transaction(&mut self, request_id: Uid) -> McOpResult<()> {
        let (op_sender, op_receiver) = oneshot::channel();
        let op = McOp::RemoveLocalPendingTransaction(request_id, op_sender);
        self.sender
            .send(op)
            .await
            .map_err(|_| McOpError::SendOpFailed)?;
        op_receiver.await.map_err(McOpError::ResponseOpFailed)?
    }
    pub async fn get_remote_pending_transaction(
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
    pub async fn insert_remote_pending_transaction(
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
    pub async fn remove_remote_pending_transaction(&mut self, request_id: Uid) -> McOpResult<()> {
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

/*
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
*/
