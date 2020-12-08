// use common::async_rpc::AsyncOpResult;
// use common::ser_utils::ser_string;
// use common::u256::U256;

use proto::crypto::PublicKey;
// use proto::funder::messages::{McBalance, PendingTransaction};

pub trait SwitchDbClient {
    type TcDbClient;
    fn tc_db_client(&mut self, friend_public_key: PublicKey) -> &mut Self::TcDbClient;

    /*
    fn get_balance(&mut self) -> AsyncOpResult<McBalance>;
    */
}
