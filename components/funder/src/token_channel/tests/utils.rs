use std::collections::HashMap;

use common::async_rpc::{AsyncOpResult, AsyncOpStream, OpError};

use proto::crypto::Signature;
use proto::funder::messages::{Currency, McBalance, MoveToken};

use database::interface::funder::CurrencyConfig;

use crate::mutual_credit::tests::MockMutualCredit;
use crate::token_channel::types::TcStatus;
use crate::token_channel::TcClient;
use crate::types::MoveTokenHashed;

#[derive(Debug)]
pub struct MockLocalResetTerms {
    reset_token: Signature,
    reset_move_token_counter: u128,
    reset_balances: Vec<(Currency, McBalance)>,
}

#[derive(Debug)]
pub struct MockRemoteResetTerms {
    reset_token: Signature,
    reset_move_token_counter: u128,
    reset_balances: Vec<(Currency, McBalance)>,
}

#[derive(Debug)]
pub enum MockTcDirection<B> {
    In(MoveTokenHashed),
    Out(MoveToken<B>, Option<MoveTokenHashed>),
}

#[derive(Debug)]
pub struct TcConsistent<B> {
    mutual_credits: HashMap<Currency, MockMutualCredit>,
    direction: MockTcDirection<B>,
}

#[derive(Debug)]
pub enum MockTcStatus<B> {
    Consistent(TcConsistent<B>),
    Inconsistent(MockLocalResetTerms, Option<MockRemoteResetTerms>),
}

#[derive(Debug)]
pub struct MockTokenChannel<B> {
    status: MockTcStatus<B>,
}

impl<B> TcClient<B> for MockTokenChannel<B>
where
    B: Clone + Send,
{
    type McClient = MockMutualCredit;

    fn mc_client(&mut self, currency: Currency) -> &mut Self::McClient {
        match &mut self.status {
            MockTcStatus::Consistent(tc_consistent) => {
                tc_consistent.mutual_credits.get_mut(&currency).unwrap()
            }
            _ => unreachable!(),
        }
    }

    fn get_tc_status(&mut self) -> AsyncOpResult<TcStatus<B>> {
        Box::pin(async move {
            Ok(match &self.status {
                MockTcStatus::Consistent(tc_consistent) => match &tc_consistent.direction {
                    MockTcDirection::In(move_token_in) => {
                        TcStatus::ConsistentIn(move_token_in.clone())
                    }
                    MockTcDirection::Out(move_token_out, opt_move_token_in) => {
                        TcStatus::ConsistentOut(move_token_out.clone(), opt_move_token_in.clone())
                    }
                },
                MockTcStatus::Inconsistent(local_reset_terms, opt_remote_reset_terms) => {
                    TcStatus::Inconsistent(
                        local_reset_terms.reset_token.clone(),
                        local_reset_terms.reset_move_token_counter,
                        opt_remote_reset_terms.as_ref().map(|remote_reset_terms| {
                            (
                                remote_reset_terms.reset_token.clone(),
                                remote_reset_terms.reset_move_token_counter.clone(),
                            )
                        }),
                    )
                }
            })
        })
    }

    fn set_direction_incoming(&mut self, move_token_hashed: MoveTokenHashed) -> AsyncOpResult<()> {
        todo!();
    }

    fn set_direction_outgoing(&mut self, move_token: MoveToken<B>) -> AsyncOpResult<()> {
        todo!();
    }

    fn set_direction_outgoing_empty_incoming(
        &mut self,
        move_token: MoveToken<B>,
    ) -> AsyncOpResult<()> {
        todo!();
    }

    fn set_inconsistent(
        &mut self,
        local_reset_token: Signature,
        local_reset_move_token_counter: u128,
    ) -> AsyncOpResult<()> {
        todo!();
    }

    /// Simulate outgoing token, to be used before an incoming reset move token (a remote reset)
    fn set_outgoing_from_inconsistent(&mut self, move_token: MoveToken<B>) -> AsyncOpResult<()> {
        todo!();
    }

    /// Simulate incoming token, to be used before an outgoing reset move token (a local reset)
    fn set_incoming_from_inconsistent(
        &mut self,
        move_token_hashed: MoveTokenHashed,
    ) -> AsyncOpResult<()> {
        todo!();
    }

    fn get_move_token_counter(&mut self) -> AsyncOpResult<u128> {
        todo!();
    }

    fn set_move_token_counter(&mut self, move_token_counter: u128) -> AsyncOpResult<()> {
        todo!();
    }

    fn get_currency_config(&mut self, currency: Currency) -> AsyncOpResult<CurrencyConfig> {
        todo!();
    }

    /// Return a sorted async iterator of all balances
    fn list_balances(&mut self) -> AsyncOpStream<(Currency, McBalance)> {
        todo!();
    }

    /// Return a sorted async iterator of all local reset proposal balances
    /// Only relevant for inconsistent channels
    fn list_local_reset_balances(&mut self) -> AsyncOpStream<(Currency, McBalance)> {
        todo!();
    }

    /// Return a sorted async iterator of all remote reset proposal balances
    /// Only relevant for inconsistent channels
    fn list_remote_reset_balances(&mut self) -> AsyncOpStream<(Currency, McBalance)> {
        todo!();
    }

    fn is_local_currency(&mut self, currency: Currency) -> AsyncOpResult<bool> {
        todo!();
    }

    fn is_remote_currency(&mut self, currency: Currency) -> AsyncOpResult<bool> {
        todo!();
    }

    fn add_local_currency(&mut self, currency: Currency) -> AsyncOpResult<()> {
        todo!();
    }

    fn remove_local_currency(&mut self, currency: Currency) -> AsyncOpResult<()> {
        todo!();
    }

    fn add_remote_currency(&mut self, currency: Currency) -> AsyncOpResult<()> {
        todo!();
    }

    fn remove_remote_currency(&mut self, currency: Currency) -> AsyncOpResult<()> {
        todo!();
    }

    fn add_mutual_credit(&mut self, currency: Currency) -> AsyncOpResult<()> {
        todo!();
    }
}
