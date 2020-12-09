// use common::async_rpc::AsyncOpResult;
// use common::ser_utils::ser_string;
// use common::u256::U256;

use crate::liveness::Liveness;
use crate::token_channel::TcDbClient;

use proto::crypto::PublicKey;
// use proto::funder::messages::{McBalance, PendingTransaction};

pub trait SwitchDbClient {
    type TcDbClient: TcDbClient;
    fn tc_db_client(&mut self, friend_public_key: PublicKey) -> &mut Self::TcDbClient;

    /*
    fn get_balance(&mut self) -> AsyncOpResult<McBalance>;
    */
}

/// Switch's ephemeral state (Not saved inside the database)
pub struct SwitchState {
    pub liveness: Liveness,
}
