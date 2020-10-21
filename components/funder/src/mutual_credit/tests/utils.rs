use common::async_rpc::OpError;
use common::conn::BoxFuture;
use std::collections::HashMap;

use proto::crypto::Uid;
use proto::funder::messages::{Currency, PendingTransaction};

use crate::mutual_credit::types::{McBalance, McTransaction};

#[derive(Arbitrary, Clone, Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct McPendingTransactions {
    /// Pending transactions that were opened locally and not yet completed
    pub local: HashMap<Uid, PendingTransaction>,
    /// Pending transactions that were opened remotely and not yet completed
    pub remote: HashMap<Uid, PendingTransaction>,
}

impl McPendingTransactions {
    fn new() -> McPendingTransactions {
        McPendingTransactions {
            local: HashMap::new(),
            remote: HashMap::new(),
        }
    }
}

#[derive(Arbitrary, Clone, Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct MutualCredit {
    /// Currency in use (How much is one credit worth?)
    pub currency: Currency,
    /// Current credit balance with respect to remote side
    pub balance: McBalance,
    /// Requests in progress
    pub pending_transactions: McPendingTransactions,
}

impl MutualCredit {
    pub fn new(
        // TODO: Should we move instead of take a reference here?
        currency: &Currency,
        balance: i128,
    ) -> MutualCredit {
        MutualCredit {
            currency: currency.clone(),
            balance: McBalance::new(balance),
            pending_transactions: McPendingTransactions::new(),
        }
    }
}

impl McTransaction for MutualCredit {
    fn get_balance(&mut self) -> BoxFuture<'static, Result<McBalance, OpError>> {
        let mc_balance = self.balance.clone();
        Box::pin(async move { Ok(mc_balance) })
    }

    fn set_balance(&mut self, balance: i128) -> BoxFuture<'static, Result<(), OpError>> {
        self.balance.balance = balance;
        Box::pin(async move { Ok(()) })
    }

    fn set_local_pending_debt(&mut self, debt: u128) -> BoxFuture<'static, Result<(), OpError>> {
        self.balance.local_pending_debt = debt;
        Box::pin(async move { Ok(()) })
    }

    fn set_remote_pending_debt(&mut self, debt: u128) -> BoxFuture<'static, Result<(), OpError>> {
        self.balance.remote_pending_debt = debt;
        Box::pin(async move { Ok(()) })
    }

    fn set_in_fees(&mut self, in_fees: u128) -> BoxFuture<'static, Result<(), OpError>> {
        self.balance.in_fees = in_fees;
        Box::pin(async move { Ok(()) })
    }

    fn set_out_fees(&mut self, out_fees: u128) -> BoxFuture<'static, Result<(), OpError>> {
        self.balance.out_fees = out_fees;
        Box::pin(async move { Ok(()) })
    }

    fn get_local_pending_transaction(
        &mut self,
        request_id: Uid,
    ) -> BoxFuture<'static, Result<Option<PendingTransaction>, OpError>> {
        let pending_transaction = self.pending_transactions.local.get(&request_id).cloned();
        Box::pin(async move { Ok(pending_transaction) })
    }

    fn insert_local_pending_transaction(
        &mut self,
        pending_transaction: PendingTransaction,
    ) -> BoxFuture<'static, Result<(), OpError>> {
        let _ = self
            .pending_transactions
            .local
            .insert(pending_transaction.request_id.clone(), pending_transaction);
        Box::pin(async move { Ok(()) })
    }

    fn remove_local_pending_transaction(
        &mut self,
        request_id: Uid,
    ) -> BoxFuture<'static, Result<(), OpError>> {
        let _ = self.pending_transactions.local.remove(&request_id);
        Box::pin(async move { Ok(()) })
    }

    fn get_remote_pending_transaction(
        &mut self,
        request_id: Uid,
    ) -> BoxFuture<'static, Result<Option<PendingTransaction>, OpError>> {
        let pending_transaction = self.pending_transactions.remote.get(&request_id).cloned();
        Box::pin(async move { Ok(pending_transaction) })
    }

    fn insert_remote_pending_transaction(
        &mut self,
        pending_transaction: PendingTransaction,
    ) -> BoxFuture<'static, Result<(), OpError>> {
        let _ = self
            .pending_transactions
            .remote
            .insert(pending_transaction.request_id.clone(), pending_transaction);
        Box::pin(async move { Ok(()) })
    }

    fn remove_remote_pending_transaction(
        &mut self,
        request_id: Uid,
    ) -> BoxFuture<'static, Result<(), OpError>> {
        let _ = self.pending_transactions.remote.remove(&request_id);
        Box::pin(async move { Ok(()) })
    }
}
