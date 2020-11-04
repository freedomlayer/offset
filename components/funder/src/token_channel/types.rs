use futures::channel::oneshot;

use common::async_rpc::{AsyncOpResult, AsyncOpStream};

use proto::crypto::Signature;
use proto::funder::messages::{Currency, McBalance, MoveToken};

use database::interface::funder::CurrencyConfig;

use crate::mutual_credit::types::McClient;
use crate::types::MoveTokenHashed;

#[derive(Debug)]
pub enum TcOpError {
    SendOpFailed,
    ResponseOpFailed(oneshot::Canceled),
}

pub type TcOpResult<T> = Result<T, TcOpError>;
pub type TcOpSenderResult<T> = oneshot::Sender<TcOpResult<T>>;

/// Status of a TokenChannel. Could be either outgoing, incoming or inconsistent.
pub enum TcStatus<B> {
    ConsistentIn(MoveTokenHashed),                        // (move_token_in)
    ConsistentOut(MoveToken<B>, Option<MoveTokenHashed>), // (move_token_out, last_move_token_in)
    Inconsistent(Signature, u128, Option<(Signature, u128)>),
    // (local_reset_token, local_reset_move_token_counter, Option<(remote_reset_token, remote_reset_move_token_counter)>)
}

pub trait TcClient<B> {
    type McClient: McClient;
    fn mc_client(&mut self, currency: Currency) -> &mut Self::McClient;

    fn get_tc_status(&mut self) -> AsyncOpResult<TcStatus<B>>;
    fn set_direction_incoming(&mut self, move_token_hashed: MoveTokenHashed) -> AsyncOpResult<()>;
    fn set_direction_outgoing(&mut self, move_token: MoveToken<B>) -> AsyncOpResult<()>;
    fn set_direction_outgoing_empty_incoming(
        &mut self,
        move_token: MoveToken<B>,
    ) -> AsyncOpResult<()>;
    fn set_inconsistent(
        &mut self,
        local_reset_token: Signature,
        local_reset_move_token_counter: u128,
    ) -> AsyncOpResult<()>;

    /// Simulate outgoing token, to be used before an incoming reset move token (a remote reset)
    fn set_outgoing_from_inconsistent(&mut self, move_token: MoveToken<B>) -> AsyncOpResult<()>;

    /// Simulate incoming token, to be used before an outgoing reset move token (a local reset)
    fn set_incoming_from_inconsistent(
        &mut self,
        move_token_hashed: MoveTokenHashed,
    ) -> AsyncOpResult<()>;

    fn get_move_token_counter(&mut self) -> AsyncOpResult<u128>;
    fn set_move_token_counter(&mut self, move_token_counter: u128) -> AsyncOpResult<()>;

    fn get_currency_config(&mut self, currency: Currency) -> AsyncOpResult<CurrencyConfig>;

    /// Return a sorted async iterator of all balances
    fn list_balances(&mut self) -> AsyncOpStream<(Currency, McBalance)>;

    /// Return a sorted async iterator of all local reset proposal balances
    /// Only relevant for inconsistent channels
    fn list_local_reset_balances(&mut self) -> AsyncOpStream<(Currency, McBalance)>;

    /// Return a sorted async iterator of all remote reset proposal balances
    /// Only relevant for inconsistent channels
    fn list_remote_reset_balances(&mut self) -> AsyncOpStream<(Currency, McBalance)>;

    fn is_local_currency(&mut self, currency: Currency) -> AsyncOpResult<bool>;
    fn is_remote_currency(&mut self, currency: Currency) -> AsyncOpResult<bool>;

    fn add_local_currency(&mut self, currency: Currency) -> AsyncOpResult<()>;
    fn remove_local_currency(&mut self, currency: Currency) -> AsyncOpResult<()>;

    fn add_remote_currency(&mut self, currency: Currency) -> AsyncOpResult<()>;
    fn remove_remote_currency(&mut self, currency: Currency) -> AsyncOpResult<()>;

    fn add_mutual_credit(&mut self, currency: Currency) -> AsyncOpResult<()>;
}
