use std::collections::HashMap;

use common::async_rpc::OpError;
use common::conn::BoxFuture;
use common::u256::U256;

use proto::crypto::{PublicKey, Uid};
use proto::funder::messages::{Currency, McBalance, PendingTransaction};

use crate::mutual_credit::incoming::{process_operation, IncomingMessage, ProcessOperationError};
use crate::mutual_credit::types::{McDbClient, McOp};

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
pub struct MockMutualCredit {
    /// Currency in use (How much is one credit worth?)
    pub currency: Currency,
    /// Current credit balance with respect to remote side
    pub balance: McBalance,
    /// Requests in progress
    pub pending_transactions: McPendingTransactions,
}

impl MockMutualCredit {
    pub fn new(
        // TODO: Should we move instead of take a reference here?
        currency: Currency,
        balance: i128,
        in_fees: U256,
        out_fees: U256,
    ) -> MockMutualCredit {
        MockMutualCredit {
            currency,
            balance: McBalance::new(balance, in_fees, out_fees),
            pending_transactions: McPendingTransactions::new(),
        }
    }
}

impl McDbClient for MockMutualCredit {
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

    fn set_in_fees(&mut self, in_fees: U256) -> BoxFuture<'static, Result<(), OpError>> {
        self.balance.in_fees = in_fees;
        Box::pin(async move { Ok(()) })
    }

    fn set_out_fees(&mut self, out_fees: U256) -> BoxFuture<'static, Result<(), OpError>> {
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

#[derive(Debug)]
pub struct ProcessTransListError {
    index: usize,
    process_trans_error: ProcessOperationError,
}

pub async fn process_operations_list(
    mc_client: &mut impl McDbClient,
    operations: Vec<McOp>,
    currency: &Currency,
    remote_public_key: &PublicKey,
    remote_max_debt: u128,
) -> Result<Vec<IncomingMessage>, ProcessTransListError> {
    let mut outputs = Vec::new();

    for (index, friend_tc_op) in operations.into_iter().enumerate() {
        match process_operation(
            mc_client,
            friend_tc_op,
            currency,
            remote_public_key,
            remote_max_debt,
        )
        .await
        {
            Err(e) => {
                return Err(ProcessTransListError {
                    index,
                    process_trans_error: e,
                })
            }
            Ok(incoming_message) => outputs.push(incoming_message),
        }
    }
    Ok(outputs)
}
