use common::async_rpc::AsyncOpResult;
// use common::ser_utils::ser_string;
use common::u256::U256;

use proto::crypto::Uid;
use proto::funder::messages::{McBalance, PendingTransaction};

pub trait McDbClient {
    fn get_balance(&mut self) -> AsyncOpResult<McBalance>;
    fn set_balance(&mut self, new_balance: i128) -> AsyncOpResult<()>;
    fn set_local_pending_debt(&mut self, debt: u128) -> AsyncOpResult<()>;
    fn set_remote_pending_debt(&mut self, debt: u128) -> AsyncOpResult<()>;
    fn set_in_fees(&mut self, in_fees: U256) -> AsyncOpResult<()>;
    fn set_out_fees(&mut self, out_fees: U256) -> AsyncOpResult<()>;
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
