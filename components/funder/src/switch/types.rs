use common::async_rpc::AsyncOpResult;
// use common::ser_utils::ser_string;
// use common::u256::U256;

use crate::liveness::Liveness;
use crate::token_channel::TcDbClient;

use proto::app_server::messages::{NamedRelayAddress, RelayAddress};
use proto::crypto::PublicKey;
// use proto::funder::messages::{McBalance, PendingTransaction};

pub trait SwitchDbClient {
    type TcDbClient: TcDbClient;
    fn tc_db_client(&mut self, friend_public_key: PublicKey) -> &mut Self::TcDbClient;

    /// Get the current list of local relays
    fn get_local_relays(&mut self) -> AsyncOpResult<Vec<NamedRelayAddress>>;

    /// Get the maximum value of sent relay generation.
    /// Returns None if all relays were acked by the remote side.
    fn get_max_sent_relays_generation(
        &mut self,
        friend_public_key: PublicKey,
    ) -> AsyncOpResult<Option<u128>>;

    /// Update sent relays for friend `friend_public_key` with the list of current local relays.
    fn update_sent_relays(
        &mut self,
        friend_public_key: PublicKey,
        generation: u128,
        sent_relays: Vec<RelayAddress>,
    ) -> AsyncOpResult<()>;

    /*
    fn get_balance(&mut self) -> AsyncOpResult<McBalance>;
    */
}

/// Switch's ephemeral state (Not saved inside the database)
pub struct SwitchState {
    pub liveness: Liveness,
}
