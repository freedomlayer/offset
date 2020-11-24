use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::mem;

use futures::{future, stream, Future};

use common::async_rpc::{AsyncOpResult, AsyncOpStream, OpError};
use common::conn::BoxFuture;
use common::safe_arithmetic::SafeSignedArithmetic;
use common::u256::U256;

use proto::crypto::{PublicKey, Signature};
use proto::funder::messages::{Currency, McBalance, MoveToken, TokenInfo};

use signature::canonical::CanonicalSerialize;

use database::interface::funder::CurrencyConfig;
use database::transaction::{TransFunc, Transaction};

use crypto::hash::hash_buffer;
use crypto::identity::compare_public_key;

use crate::mutual_credit::tests::MockMutualCredit;
use crate::token_channel::types::{ResetBalance, ResetTerms, TcStatus};
use crate::token_channel::{initial_move_token, reset_balance_to_mc_balance, TcClient};
use crate::types::{create_hashed, MoveTokenHashed};

#[derive(Debug, Clone)]
pub enum MockTcDirection {
    In(MoveTokenHashed),
    Out(MoveToken, Option<MoveTokenHashed>),
}

#[derive(Debug, Clone)]
pub struct TcConsistent {
    mutual_credits: HashMap<Currency, MockMutualCredit>,
    direction: MockTcDirection,
    move_token_counter: u128,
    local_currencies: HashSet<Currency>,
    remote_currencies: HashSet<Currency>,
}

#[derive(Debug, Clone)]
pub enum MockTcStatus {
    Consistent(TcConsistent),
    Inconsistent(ResetTerms, Option<ResetTerms>),
}

#[derive(Debug, Clone)]
pub struct MockTokenChannel {
    status: MockTcStatus,
    /// Remote max debt, configured for each currency
    /// (And possibly for currencies that are not yet active)
    pub remote_max_debts: HashMap<Currency, u128>,
}

impl MockTokenChannel {
    pub fn new(local_public_key: &PublicKey, remote_public_key: &PublicKey) -> Self {
        // First move token message for both sides
        let move_token_counter = 0;
        if compare_public_key(&local_public_key, &remote_public_key) == Ordering::Less {
            // We are the first sender
            MockTokenChannel {
                status: MockTcStatus::Consistent(TcConsistent {
                    mutual_credits: HashMap::new(),
                    direction: MockTcDirection::Out(
                        initial_move_token(local_public_key, remote_public_key),
                        None,
                    ),
                    move_token_counter,
                    local_currencies: HashSet::new(),
                    remote_currencies: HashSet::new(),
                }),
                remote_max_debts: HashMap::new(),
            }
        } else {
            // We are the second sender
            let move_token_in = initial_move_token(remote_public_key, local_public_key);
            let token_info = TokenInfo {
                // No balances yet:
                balances_hash: hash_buffer(&[]),
                move_token_counter,
            };

            MockTokenChannel {
                status: MockTcStatus::Consistent(TcConsistent {
                    mutual_credits: HashMap::new(),
                    direction: MockTcDirection::In(create_hashed(&move_token_in, &token_info)),
                    move_token_counter,
                    local_currencies: HashSet::new(),
                    remote_currencies: HashSet::new(),
                }),
                remote_max_debts: HashMap::new(),
            }
        }
    }
}

/// Calculate ResetBalance for a specific mutual credit
fn calc_reset_balance(mock_token_channel: &MockMutualCredit) -> ResetBalance {
    let mc_balance = &mock_token_channel.balance;

    // Calculate in_fees, adding fees from remote pending requests:
    let mut in_fees = mc_balance.in_fees;
    for (_uid, pending_transaction) in &mock_token_channel.pending_transactions.remote {
        in_fees
            .checked_add(U256::from(pending_transaction.left_fees))
            .unwrap();
    }

    ResetBalance {
        // Calculate reset balance, including pending debt
        balance: mc_balance
            .balance
            .checked_add_unsigned(mc_balance.remote_pending_debt)
            .unwrap(),
        in_fees,
        out_fees: mc_balance.out_fees,
    }
}

impl TcClient for MockTokenChannel {
    type McClient = MockMutualCredit;

    fn mc_client(&mut self, currency: Currency) -> &mut Self::McClient {
        match &mut self.status {
            MockTcStatus::Consistent(tc_consistent) => {
                tc_consistent.mutual_credits.get_mut(&currency).unwrap()
            }
            _ => unreachable!(),
        }
    }

    fn get_tc_status(&mut self) -> AsyncOpResult<TcStatus> {
        let res = Ok(match &self.status {
            MockTcStatus::Consistent(tc_consistent) => match &tc_consistent.direction {
                MockTcDirection::In(move_token_in) => TcStatus::ConsistentIn(move_token_in.clone()),
                MockTcDirection::Out(move_token_out, opt_move_token_in) => {
                    TcStatus::ConsistentOut(move_token_out.clone(), opt_move_token_in.clone())
                }
            },
            MockTcStatus::Inconsistent(local_reset_terms, opt_remote_reset_terms) => {
                TcStatus::Inconsistent(
                    local_reset_terms.reset_token.clone(),
                    local_reset_terms.move_token_counter,
                    opt_remote_reset_terms.as_ref().map(|remote_reset_terms| {
                        (
                            remote_reset_terms.reset_token.clone(),
                            remote_reset_terms.move_token_counter.clone(),
                        )
                    }),
                )
            }
        });
        Box::pin(future::ready(res))
    }

    fn set_direction_incoming(&mut self, move_token_hashed: MoveTokenHashed) -> AsyncOpResult<()> {
        let tc_consistent = match &mut self.status {
            MockTcStatus::Consistent(tc_consistent) => tc_consistent,
            _ => unreachable!(),
        };

        tc_consistent.move_token_counter = move_token_hashed.token_info.move_token_counter;
        tc_consistent.direction = MockTcDirection::In(move_token_hashed);
        Box::pin(future::ready(Ok(())))
    }

    fn set_direction_outgoing(
        &mut self,
        move_token: MoveToken,
        move_token_counter: u128,
    ) -> AsyncOpResult<()> {
        let tc_consistent = match &mut self.status {
            MockTcStatus::Consistent(tc_consistent) => tc_consistent,
            _ => unreachable!(),
        };

        // Set `move_token_counter`:
        tc_consistent.move_token_counter = move_token_counter;

        let last_move_token_in = match &tc_consistent.direction {
            MockTcDirection::In(move_token_in) => move_token_in.clone(),
            _ => unreachable!(),
        };

        tc_consistent.direction = MockTcDirection::Out(move_token, Some(last_move_token_in));
        Box::pin(future::ready(Ok(())))
    }

    fn set_direction_outgoing_empty_incoming(
        &mut self,
        move_token: MoveToken,
        move_token_counter: u128,
    ) -> AsyncOpResult<()> {
        let tc_consistent = match &mut self.status {
            MockTcStatus::Consistent(tc_consistent) => tc_consistent,
            _ => unreachable!(),
        };

        tc_consistent.direction = MockTcDirection::Out(move_token, None);
        Box::pin(future::ready(Ok(())))
    }

    fn set_inconsistent(
        &mut self,
        local_reset_token: Signature,
        local_reset_move_token_counter: u128,
    ) -> AsyncOpResult<()> {
        // Calculate `reset_balances`:
        let reset_balances: HashMap<_, _> = match &mut self.status {
            MockTcStatus::Consistent(tc_consistent) => tc_consistent
                .mutual_credits
                .iter()
                .map(|(currency, mock_mutual_credit)| {
                    (currency.clone(), calc_reset_balance(&mock_mutual_credit))
                })
                .collect(),
            MockTcStatus::Inconsistent(..) => unreachable!(),
        };

        // Change status to inconsistent:
        self.status = MockTcStatus::Inconsistent(
            ResetTerms {
                reset_token: local_reset_token,
                move_token_counter: local_reset_move_token_counter,
                reset_balances,
            },
            None,
        );
        Box::pin(future::ready(Ok(())))
    }

    /// Set remote terms for reset. Can only be called if we are in inconsistent state.
    fn set_inconsistent_remote_terms(
        &mut self,
        remote_reset_token: Signature,
        remote_reset_move_token_counter: u128,
    ) -> AsyncOpResult<()> {
        let local_reset_terms = match &self.status {
            MockTcStatus::Consistent(..) | MockTcStatus::Inconsistent(_, Some(_)) => unreachable!(),
            MockTcStatus::Inconsistent(local_reset_terms, None) => local_reset_terms.clone(),
        };

        self.status = MockTcStatus::Inconsistent(
            local_reset_terms,
            Some(ResetTerms {
                reset_token: remote_reset_token,
                move_token_counter: remote_reset_move_token_counter,
                // Note that reset_balances is currently empty, and needs to filled.
                reset_balances: HashMap::new(),
            }),
        );

        Box::pin(future::ready(Ok(())))
    }

    fn add_remote_reset_balance(
        &mut self,
        currency: Currency,
        reset_balance: ResetBalance,
    ) -> AsyncOpResult<()> {
        let remote_reset_terms = match &mut self.status {
            MockTcStatus::Consistent(..) | MockTcStatus::Inconsistent(_, None) => unreachable!(),
            MockTcStatus::Inconsistent(local_reset_terms, Some(remote_reset_terms)) => {
                remote_reset_terms
            }
        };

        if let Some(_) = remote_reset_terms
            .reset_balances
            .insert(currency, reset_balance)
        {
            unreachable!();
        }

        Box::pin(future::ready(Ok(())))
    }

    /// Simulate outgoing token, to be used before an incoming reset move token (a remote reset)
    fn set_outgoing_from_inconsistent(&mut self, move_token: MoveToken) -> AsyncOpResult<()> {
        let local_reset_terms = match &self.status {
            MockTcStatus::Consistent(..) => unreachable!(),
            MockTcStatus::Inconsistent(local_reset_terms, _opt_remote_reset_terms) => {
                local_reset_terms.clone()
            }
        };

        let mutual_credits = local_reset_terms
            .reset_balances
            .iter()
            .map(|(currency, reset_balance)| {
                (
                    currency.clone(),
                    MockMutualCredit::new(
                        currency.clone(),
                        reset_balance.balance,
                        reset_balance.in_fees,
                        reset_balance.out_fees,
                    ),
                )
            })
            .collect();

        let currencies_set: HashSet<_> = local_reset_terms
            .reset_balances
            .iter()
            .map(|(currency, _)| currency)
            .cloned()
            .collect();

        self.status = MockTcStatus::Consistent(TcConsistent {
            mutual_credits,
            direction: MockTcDirection::Out(move_token, None),
            move_token_counter: local_reset_terms.move_token_counter.checked_sub(1).unwrap(),
            local_currencies: currencies_set.clone(),
            remote_currencies: currencies_set,
        });

        Box::pin(future::ready(Ok(())))
    }

    /// Simulate incoming token, to be used before an outgoing reset move token (a local reset)
    fn set_incoming_from_inconsistent(
        &mut self,
        move_token_hashed: MoveTokenHashed,
    ) -> AsyncOpResult<()> {
        let remote_reset_terms = match &self.status {
            MockTcStatus::Consistent(..) => unreachable!(),
            MockTcStatus::Inconsistent(_, None) => unreachable!(),
            MockTcStatus::Inconsistent(local_reset_terms, Some(remote_reset_terms)) => {
                remote_reset_terms.clone()
            }
        };

        let mutual_credits = remote_reset_terms
            .reset_balances
            .iter()
            .map(|(currency, reset_balance)| {
                (
                    currency.clone(),
                    MockMutualCredit::new(
                        currency.clone(),
                        // Note: direction is flipped:
                        reset_balance.balance.checked_neg().unwrap(),
                        reset_balance.out_fees,
                        reset_balance.in_fees,
                    ),
                )
            })
            .collect();

        let currencies_set: HashSet<_> = remote_reset_terms
            .reset_balances
            .iter()
            .map(|(currency, _)| currency)
            .cloned()
            .collect();

        self.status = MockTcStatus::Consistent(TcConsistent {
            mutual_credits,
            direction: MockTcDirection::In(move_token_hashed),
            move_token_counter: remote_reset_terms
                .move_token_counter
                .checked_sub(1)
                .unwrap(),
            local_currencies: currencies_set.clone(),
            remote_currencies: currencies_set,
        });

        Box::pin(future::ready(Ok(())))
    }

    fn get_move_token_counter(&mut self) -> AsyncOpResult<u128> {
        Box::pin(future::ready(Ok(match &self.status {
            MockTcStatus::Consistent(tc_consistent) => tc_consistent.move_token_counter,
            MockTcStatus::Inconsistent(..) => unreachable!(),
        })))
    }

    fn get_remote_max_debt(&mut self, currency: Currency) -> AsyncOpResult<u128> {
        Box::pin(future::ready(Ok(*self
            .remote_max_debts
            .get(&currency)
            .unwrap())))
    }

    /// Return a sorted async iterator of all balances
    fn list_balances(&mut self) -> AsyncOpStream<(Currency, McBalance)> {
        let tc_consistent = match &self.status {
            MockTcStatus::Consistent(tc_consistent) => tc_consistent,
            MockTcStatus::Inconsistent(..) => unreachable!(),
        };

        let mut balances_vec: Vec<_> = tc_consistent
            .mutual_credits
            .iter()
            .map(|(currency, mutual_credit)| (currency.clone(), mutual_credit.balance.clone()))
            .collect();

        balances_vec.sort_by(|(currency1, _), (currency2, _)| currency1.cmp(currency2));
        Box::pin(stream::iter(balances_vec.into_iter().map(|item| Ok(item))))
    }

    /// Return a sorted async iterator of all local reset proposal balances
    /// Only relevant for inconsistent channels
    fn list_local_reset_balances(&mut self) -> AsyncOpStream<(Currency, ResetBalance)> {
        let local_reset_terms = match &self.status {
            MockTcStatus::Consistent(..) => unreachable!(),
            MockTcStatus::Inconsistent(local_reset_terms, _opt_remote_reset_terms) => {
                local_reset_terms
            }
        };

        let mut balances_vec: Vec<_> = local_reset_terms
            .reset_balances
            .iter()
            .map(|(currency, reset_balance)| (currency.clone(), reset_balance.clone()))
            .collect();

        balances_vec.sort_by(|(currency1, _), (currency2, _)| currency1.cmp(currency2));
        Box::pin(stream::iter(balances_vec.into_iter().map(|item| Ok(item))))
    }

    /// Return a sorted async iterator of all remote reset proposal balances
    /// Only relevant for inconsistent channels
    fn list_remote_reset_balances(&mut self) -> AsyncOpStream<(Currency, ResetBalance)> {
        let remote_reset_terms = match &self.status {
            MockTcStatus::Consistent(..) | MockTcStatus::Inconsistent(_, None) => unreachable!(),
            MockTcStatus::Inconsistent(_local_reset_terms, Some(remote_reset_terms)) => {
                remote_reset_terms
            }
        };

        let mut balances_vec: Vec<_> = remote_reset_terms
            .reset_balances
            .iter()
            .map(|(currency, reset_balance)| (currency.clone(), reset_balance.clone()))
            .collect();

        balances_vec.sort_by(|(currency1, _), (currency2, _)| currency1.cmp(currency2));
        Box::pin(stream::iter(balances_vec.into_iter().map(|item| Ok(item))))
    }

    fn is_local_currency(&mut self, currency: Currency) -> AsyncOpResult<bool> {
        let local_currencies = match &self.status {
            MockTcStatus::Consistent(tc_consistent) => &tc_consistent.local_currencies,
            MockTcStatus::Inconsistent(..) => unreachable!(),
        };
        Box::pin(future::ready(Ok(local_currencies.contains(&currency))))
    }

    fn is_remote_currency(&mut self, currency: Currency) -> AsyncOpResult<bool> {
        let remote_currencies = match &self.status {
            MockTcStatus::Consistent(tc_consistent) => &tc_consistent.remote_currencies,
            MockTcStatus::Inconsistent(..) => unreachable!(),
        };
        Box::pin(future::ready(Ok(remote_currencies.contains(&currency))))
    }

    fn add_local_currency(&mut self, currency: Currency) -> AsyncOpResult<bool> {
        Box::pin(future::ready(Ok(match &mut self.status {
            MockTcStatus::Consistent(tc_consistent) => {
                tc_consistent.local_currencies.insert(currency)
            }
            MockTcStatus::Inconsistent(..) => unreachable!(),
        })))
    }

    fn remove_local_currency(&mut self, currency: Currency) -> AsyncOpResult<bool> {
        Box::pin(future::ready(Ok(match &mut self.status {
            MockTcStatus::Consistent(tc_consistent) => {
                tc_consistent.local_currencies.remove(&currency)
            }
            MockTcStatus::Inconsistent(..) => unreachable!(),
        })))
    }

    fn add_remote_currency(&mut self, currency: Currency) -> AsyncOpResult<bool> {
        Box::pin(future::ready(Ok(match &mut self.status {
            MockTcStatus::Consistent(tc_consistent) => {
                tc_consistent.remote_currencies.insert(currency)
            }
            MockTcStatus::Inconsistent(..) => unreachable!(),
        })))
    }

    fn remove_remote_currency(&mut self, currency: Currency) -> AsyncOpResult<bool> {
        Box::pin(future::ready(Ok(match &mut self.status {
            MockTcStatus::Consistent(tc_consistent) => {
                tc_consistent.remote_currencies.remove(&currency)
            }
            MockTcStatus::Inconsistent(..) => unreachable!(),
        })))
    }

    fn add_mutual_credit(&mut self, currency: Currency) -> AsyncOpResult<()> {
        let balance = 0;
        let in_fees = 0.into();
        let out_fees = 0.into();
        match &mut self.status {
            MockTcStatus::Consistent(tc_consistent) => {
                let res = tc_consistent.mutual_credits.insert(
                    currency.clone(),
                    MockMutualCredit::new(currency, balance, in_fees, out_fees),
                );
                if let Some(_) = res {
                    unreachable!();
                }
            }
            MockTcStatus::Inconsistent(..) => unreachable!(),
        };
        Box::pin(future::ready(Ok(())))
    }
}

impl Transaction for MockTokenChannel {
    fn transaction<'a, F, T, E>(&'a mut self, f: F) -> BoxFuture<'a, Result<T, E>>
    where
        F: TransFunc<InRef = Self, Out = Result<T, E>> + Send + 'a,
        T: Send,
        E: Send,
    {
        Box::pin(async move {
            // Save original value, before we start making modifications:
            let orig_mock_token_channel = self.clone();
            match f.call(self).await {
                Ok(t) => {
                    // Transaction was successful
                    Ok(t)
                }
                Err(e) => {
                    // Cancel transaction
                    let _ = mem::replace(self, orig_mock_token_channel);
                    Err(e)
                }
            }
        })
    }
}
