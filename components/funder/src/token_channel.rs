use std::cmp::Ordering;
use std::collections::HashMap as ImHashMap;
use std::convert::TryFrom;

use derive_more::From;

use futures::channel::{mpsc, oneshot};
use futures::SinkExt;
use futures::Stream;

use common::async_rpc::{AsyncOpResult, AsyncOpStream, OpError};
use common::conn::BoxFuture;
use common::{get_out_type, ops_trait};

use im::hashset::HashSet as ImHashSet;

// use common::ser_utils::ser_map_str_any;

use signature::canonical::CanonicalSerialize;

use crypto::hash::sha_512_256;
use crypto::identity::compare_public_key;

use proto::crypto::{HashResult, PublicKey, RandValue, Signature, Uid};

use proto::app_server::messages::RelayAddress;
use proto::funder::messages::{
    BalanceInfo, CountersInfo, Currency, CurrencyBalanceInfo, CurrencyOperations, McInfo,
    MoveToken, PendingTransaction, TokenInfo, UnsignedMoveToken,
};
use signature::signature_buff::hash_token_info;
use signature::verify::verify_move_token;

use database::interface::funder::CurrencyConfig;

use crate::mutual_credit::incoming::{
    process_operations_list, IncomingMessage, ProcessTransListError,
};
use crate::mutual_credit::outgoing::queue_operation;
use crate::mutual_credit::types::{McBalance, McOp, McTransaction};

use crate::types::{create_hashed, create_unsigned_move_token, MoveTokenHashed};

/*
#[derive(Arbitrary, Debug, Clone, Serialize, Deserialize)]
pub enum SetDirection<B> {
    Incoming(MoveTokenHashed),
    Outgoing((MoveToken<B>, TokenInfo)),
}
*/

#[derive(Debug)]
pub enum TcOpError {
    SendOpFailed,
    ResponseOpFailed(oneshot::Canceled),
}

pub type TcOpResult<T> = Result<T, TcOpError>;
pub type TcOpSenderResult<T> = oneshot::Sender<TcOpResult<T>>;

pub enum TcStatus<B> {
    ConsistentIn(MoveTokenHashed),
    ConsistentOut(MoveToken<B>, MoveTokenHashed),
    Inconsistent,
}

pub struct MutualCreditInfo {
    pub balance: i128,
    pub local_pending_debt: u128,
    pub remote_pending_debt: u128,
    pub in_fees: u128,
    pub out_fees: u128,
}

pub trait TcTransaction<B> {
    type McTransaction: McTransaction;
    fn mc_transaction(&mut self, currency: Currency) -> &mut Self::McTransaction;

    fn get_tc_status(&mut self) -> AsyncOpResult<TcStatus<B>>;
    fn set_direction_incoming(&mut self, move_token_hashed: MoveTokenHashed) -> AsyncOpResult<()>;
    fn set_direction_outgoing(&mut self, move_token: MoveToken<B>) -> AsyncOpResult<()>;

    // get_move_token_in() -> Option<MoveTokenHashed>;
    // get_move_token_out() -> Option<MoveToken<B>>;

    fn get_move_token_counter(&mut self) -> u128;
    fn get_currency_config(&mut self, currency: Currency) -> AsyncOpResult<CurrencyConfig>;

    /// Return a sorted list of all mutual credits
    fn list_mutual_credits(&mut self) -> AsyncOpStream<MutualCreditInfo>;

    // add_local_currency(currency: Currency);
    // TODO: Possibly add boolean result here? And to other remove commands?
    // remove_local_currency(currency: Currency);

    fn is_local_currency(&mut self, currency: Currency) -> AsyncOpResult<bool>;
    fn is_remote_currency(&mut self, currency: Currency) -> AsyncOpResult<bool>;

    fn add_remote_currency(&mut self, currency: Currency) -> AsyncOpResult<()>;
    fn remove_remote_currency(&mut self, currency: Currency) -> AsyncOpResult<()>;

    // get_remote_active_currencies() ->
    // set_remote_active_currencies(currencies: Vec<Currency>);
    // is_active_currency(currency: Currency) -> bool;

    fn add_mutual_credit(&mut self, currency: Currency) -> AsyncOpResult<()>;
}

/// The currencies set to be active by two sides of the token channel.
/// Only currencies that are active on both sides (Intersection) can be used for trading.
#[derive(Arbitrary, Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ActiveCurrencies {
    /// Currencies set to be active by local side
    pub local: ImHashSet<Currency>,
    /// Currencies set to be active by remote side
    pub remote: ImHashSet<Currency>,
}

impl ActiveCurrencies {
    fn new() -> Self {
        Self {
            local: ImHashSet::new(),
            remote: ImHashSet::new(),
        }
    }

    pub fn calc_active(&self) -> ImHashSet<Currency> {
        self.local.clone().intersection(self.remote.clone())
    }
}

#[derive(Arbitrary, Clone, Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct TcOutgoing<B> {
    pub move_token_out: MoveToken<B>,
    pub token_info: TokenInfo,
    pub opt_prev_move_token_in: Option<MoveTokenHashed>,
}

#[derive(Arbitrary, Clone, Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct TcIncoming {
    pub move_token_in: MoveTokenHashed,
}

#[allow(clippy::large_enum_variant)]
#[derive(Arbitrary, Clone, Serialize, Deserialize, Debug, PartialEq, Eq)]
pub enum TcDirection<B> {
    Incoming(TcIncoming),
    Outgoing(TcOutgoing<B>),
}

/*
#[derive(Clone, Debug)]
pub struct TcInBorrow<'a> {
    pub tc_incoming: &'a TcIncoming,
    mutual_credits: &'a ImHashMap<Currency, MutualCredit>,
    active_currencies: &'a ActiveCurrencies,
}

#[derive(Clone, Debug)]
pub struct TcOutBorrow<'a, B> {
    pub tc_outgoing: &'a TcOutgoing<B>,
    mutual_credits: &'a ImHashMap<Currency, MutualCredit>,
    active_currencies: &'a ActiveCurrencies,
}

#[derive(Clone, Debug)]
pub enum TcDirectionBorrow<'a, B> {
    In(TcInBorrow<'a>),
    Out(TcOutBorrow<'a, B>),
}
*/

/*
#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct TokenChannel<B> {
    // direction: TcDirection<B>,
    tc_transaction: TcTransaction<B>,
    // mutual_credits: ImHashMap<Currency, MutualCredit>,
    // active_currencies: ActiveCurrencies,
}
*/

#[derive(Debug, From)]
pub enum ReceiveMoveTokenError {
    ChainInconsistency,
    InvalidTransaction(ProcessTransListError),
    InvalidSignature,
    InvalidTokenInfo,
    InvalidInconsistencyCounter,
    MoveTokenCounterOverflow,
    InvalidMoveTokenCounter,
    TooManyOperations,
    InvalidCurrency,
    InvalidAddActiveCurrencies,
    CanNotRemoveCurrencyInUse,
    InvalidState,
    OpError(OpError),
}

#[derive(Debug)]
pub struct MoveTokenReceivedCurrency {
    pub currency: Currency,
    pub incoming_messages: Vec<IncomingMessage>,
}

#[derive(Debug)]
pub struct MoveTokenReceived<B> {
    // pub mutations: Vec<TcMutation<B>>,
    pub currencies: Vec<MoveTokenReceivedCurrency>,
    pub relays_diff: Vec<RelayAddress<B>>,
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
pub enum ReceiveMoveTokenOutput<B> {
    Duplicate,
    RetransmitOutgoing(MoveToken<B>),
    Received(MoveTokenReceived<B>),
    // In case of a reset, all the local pending requests will be canceled.
}

#[derive(Debug)]
pub struct SendMoveTokenOutput<B> {
    pub unsigned_move_token: UnsignedMoveToken<B>,
    // pub mutations: Vec<TcMutation<B>>,
    pub token_info: TokenInfo,
}

#[derive(Debug)]
pub enum SendMoveTokenError {
    LocalCurrencyAlreadyExists,
    CanNotRemoveCurrencyInUse,
}

/// Create a token from a public key
/// Currently this function puts the public key in the beginning of the signature buffer,
/// as the public key is shorter than a signature.
/// Possibly change this in the future (Maybe use a hash to spread the public key over all the
/// bytes of the signature)
///
/// Note that the output here is not a real signature. This function is used for the first
/// deterministic initialization of a token channel.
fn token_from_public_key(public_key: &PublicKey) -> Signature {
    let mut buff = [0; Signature::len()];
    buff[0..PublicKey::len()].copy_from_slice(public_key);
    Signature::from(&buff)
}

/// Generate a random nonce from public key.
/// Note that the result here is not really a random nonce. This function is used for the first
/// deterministic initialization of a token channel.
fn rand_nonce_from_public_key(public_key: &PublicKey) -> RandValue {
    let public_key_hash = sha_512_256(public_key);
    RandValue::try_from(&public_key_hash.as_ref()[..RandValue::len()]).unwrap()
}

/// Create an initial move token in the relationship between two public keys.
/// To canonicalize the initial move token (Having an equal move token for both sides), we sort the
/// two public keys in some way.
fn initial_move_token<B>(
    low_public_key: &PublicKey,
    high_public_key: &PublicKey,
) -> (MoveToken<B>, TokenInfo) {
    let token_info = TokenInfo {
        mc: McInfo {
            local_public_key: low_public_key.clone(),
            remote_public_key: high_public_key.clone(),
            balances: Vec::new(),
        },
        counters: CountersInfo {
            inconsistency_counter: 0,
            move_token_counter: 0,
        },
    };

    // This is a special initialization case.
    // Note that this is the only case where new_token is not a valid signature.
    // We do this because we want to have synchronization between the two sides of the token
    // channel, however, the remote side has no means of generating the signature (Because he
    // doesn't have the private key). Therefore we use a dummy new_token instead.
    let move_token = MoveToken {
        old_token: token_from_public_key(&low_public_key),
        currencies_operations: Vec::new(),
        relays_diff: Vec::new(),
        currencies_diff: Vec::new(),
        info_hash: hash_token_info(&token_info),
        new_token: token_from_public_key(&high_public_key),
    };

    (move_token, token_info)
}

async fn handle_in_move_token<B>(
    tc_transaction: &mut impl TcTransaction<B>,
    new_move_token: MoveToken<B>,
    remote_public_key: &PublicKey,
) -> Result<ReceiveMoveTokenOutput<B>, ReceiveMoveTokenError>
where
    B: CanonicalSerialize + Clone,
{
    match tc_transaction.get_tc_status().await? {
        TcStatus::ConsistentIn(move_token_in) => {
            handle_in_move_token_dir_in(tc_transaction, move_token_in, new_move_token).await
        }
        TcStatus::ConsistentOut(move_token_out, _move_token_in) => {
            handle_in_move_token_dir_out(
                tc_transaction,
                move_token_out,
                new_move_token,
                remote_public_key,
            )
            .await
        }
        TcStatus::Inconsistent => unreachable!(),
    }
}

async fn handle_in_move_token_dir_in<B>(
    tc_transaction: &mut impl TcTransaction<B>,
    move_token_in: MoveTokenHashed,
    new_move_token: MoveToken<B>,
) -> Result<ReceiveMoveTokenOutput<B>, ReceiveMoveTokenError>
where
    B: CanonicalSerialize + Clone,
{
    if move_token_in == create_hashed(&new_move_token, &move_token_in.token_info) {
        // Duplicate
        Ok(ReceiveMoveTokenOutput::Duplicate)
    } else {
        // Inconsistency
        Err(ReceiveMoveTokenError::ChainInconsistency)
    }
}

async fn handle_in_move_token_dir_out<B>(
    tc_transaction: &mut impl TcTransaction<B>,
    move_token_out: MoveToken<B>,
    new_move_token: MoveToken<B>,
    remote_public_key: &PublicKey,
) -> Result<ReceiveMoveTokenOutput<B>, ReceiveMoveTokenError>
where
    B: Clone + CanonicalSerialize,
{
    if new_move_token.old_token == move_token_out.new_token {
        Ok(ReceiveMoveTokenOutput::Received(
            handle_incoming_token_match(
                tc_transaction,
                move_token_out,
                new_move_token,
                remote_public_key,
            )
            .await?,
        ))
    // self.outgoing_to_incoming(friend_move_token, new_move_token)
    } else if move_token_out.old_token == new_move_token.new_token {
        // We should retransmit our move token message to the remote side.
        Ok(ReceiveMoveTokenOutput::RetransmitOutgoing(
            move_token_out.clone(),
        ))
    } else {
        Err(ReceiveMoveTokenError::ChainInconsistency)
    }
}

async fn hash_token_info(
    local_public_key: PublicKey,
    remote_public_key: PublicKey,
    move_token_counter: u128,
    mutual_credit_infos: impl Stream<Item = MutualCreditInfo> + Unpin,
) -> HashResult {
}

async fn handle_incoming_token_match<B>(
    tc_transaction: &mut impl TcTransaction<B>,
    move_token_out: MoveToken<B>,
    new_move_token: MoveToken<B>, // remote_max_debts: &ImHashMap<Currency, u128>,
    remote_public_key: &PublicKey,
) -> Result<MoveTokenReceived<B>, ReceiveMoveTokenError>
where
    B: Clone + CanonicalSerialize,
{
    // Verify signature:
    // Note that we only verify the signature here, and not at the Incoming part.
    // This allows the genesis move token to occur smoothly, even though its signature
    // is not correct.
    // TODO: Check if the above statement is still true.
    if !verify_move_token(new_move_token.clone(), &remote_public_key) {
        return Err(ReceiveMoveTokenError::InvalidSignature);
    }

    // Aggregate results for every currency:
    let mut move_token_received = MoveTokenReceived {
        currencies: Vec::new(),
        relays_diff: new_move_token.relays_diff.clone(),
    };

    // Handle active currencies:

    // currencies_diff represents a "xor" between the previous set of currencies and the new set of
    // currencies.
    //
    // TODO:
    // - Add to remote currencies every currency that is in the xor set, but not in the current
    // set.
    // - Remove from remote currencies currencies that satisfy the following requirements:
    // - (1) in the xor set.
    // - (2) in the remote
    // - (3) (Not in the local set) or (in the local set with balance zero and zero pending debts)
    for diff_currency in &new_move_token.currencies_diff {
        // Check if we need to add currency:
        if !tc_transaction
            .is_remote_currency(diff_currency.clone())
            .await?
        {
            // Add a new remote currency.
            tc_transaction
                .add_remote_currency(diff_currency.clone())
                .await?;

            // If we also have this currency as a local currency, add a new mutual credit:
            if tc_transaction
                .is_local_currency(diff_currency.clone())
                .await?
            {
                tc_transaction.add_mutual_credit(diff_currency.clone());
            }
        } else {
            let is_local_currency = tc_transaction
                .is_local_currency(diff_currency.clone())
                .await?;
            if is_local_currency {
                let mc_balance = tc_transaction
                    .mc_transaction(diff_currency.clone())
                    .get_balance()
                    .await?;
                if mc_balance.balance == 0
                    && mc_balance.remote_pending_debt == 0
                    && mc_balance.local_pending_debt == 0
                {
                    // We may remove the currency if the balance and pending debts are exactly
                    // zero:
                    tc_transaction
                        .remove_remote_currency(diff_currency.clone())
                        .await?;
                    // TODO: We need to report outside that we removed this currency somehow.
                    // TODO: Somehow unite all code for removing remote currencies.
                    todo!();
                } else {
                    // We are not allowed to remove this currency!
                    return Err(ReceiveMoveTokenError::CanNotRemoveCurrencyInUse);
                }
            } else {
                tc_transaction
                    .remove_remote_currency(diff_currency.clone())
                    .await?;
                // TODO: We need to report outside that we removed this currency somehow.
                todo!();
            }
        }
    }

    // TODO: How do we pass the new relay's information? Do we need to update anything related to
    // the new relays?

    // Attempt to apply operations for every currency:
    for currency_operations in &new_move_token.currencies_operations {
        let remote_max_debt = tc_transaction
            .get_currency_config(currency_operations.currency.clone())
            .await?
            .remote_max_debt;

        let incoming_messages = process_operations_list(
            tc_transaction.mc_transaction(currency_operations.currency.clone()),
            currency_operations.operations.clone(),
            &currency_operations.currency,
            remote_public_key,
            remote_max_debt,
        )
        .await
        .map_err(ReceiveMoveTokenError::InvalidTransaction)?;

        // We apply mutations on this token channel, to verify stated balance values
        // let mut check_mutual_credit = mutual_credit.clone();
        let move_token_received_currency = MoveTokenReceivedCurrency {
            currency: currency_operations.currency.clone(),
            incoming_messages,
        };

        move_token_received
            .currencies
            .push(move_token_received_currency);
    }

    // Create what we expect to be TokenInfo (From the point of view of remote side):
    let mutual_credit_infos = tc_transaction.list_mutual_credits();
    while let Some(mutual_credit_info) = mutual_credit_infos.next().await {}

    let tc_out_borrow = token_channel.get_outgoing().unwrap();
    let mut expected_balances: Vec<_> = tc_out_borrow
        .mutual_credits
        .iter()
        .map(|(currency, mc)| CurrencyBalanceInfo {
            currency: currency.clone(),
            balance_info: BalanceInfo {
                balance: mc.state().balance.balance,
                local_pending_debt: mc.state().balance.local_pending_debt,
                remote_pending_debt: mc.state().balance.remote_pending_debt,
            },
        })
        .collect();

    // Canonicalize:
    expected_balances.sort_by(|cbi1, cbi2| cbi1.currency.cmp(&cbi2.currency));

    let expected_token_info = TokenInfo {
        mc: McInfo {
            local_public_key: tc_out_borrow
                .tc_outgoing
                .token_info
                .mc
                .local_public_key
                .clone(),
            remote_public_key: tc_out_borrow
                .tc_outgoing
                .token_info
                .mc
                .remote_public_key
                .clone(),
            balances: expected_balances,
        },
        counters: CountersInfo {
            inconsistency_counter: tc_out_borrow
                .tc_outgoing
                .token_info
                .counters
                .inconsistency_counter,
            move_token_counter: tc_out_borrow
                .tc_outgoing
                .token_info
                .counters
                .move_token_counter
                .checked_add(1)
                .ok_or(ReceiveMoveTokenError::MoveTokenCounterOverflow)?,
        },
    }
    .flip();

    // Verify stated balances:
    let info_hash = hash_token_info(&expected_token_info);
    if new_move_token.info_hash != info_hash {
        return Err(ReceiveMoveTokenError::InvalidTokenInfo);
    }

    move_token_received
        .mutations
        .push(TcMutation::SetDirection(SetDirection::Incoming(
            create_hashed(&new_move_token, &expected_token_info),
        )));

    Ok(move_token_received)
}

fn handle_out_move_token<B>(tc_transaction: &mut impl TcTransaction<B>) {
    todo!();
}

/*
impl<B> TokenChannel<B>
where
    B: Clone + CanonicalSerialize,
{
    pub fn new(local_public_key: &PublicKey, remote_public_key: &PublicKey) -> Self {
        if compare_public_key(&local_public_key, &remote_public_key) == Ordering::Less {
            // We are the first sender
            let (move_token_out, token_info) =
                initial_move_token(local_public_key, remote_public_key);
            let tc_outgoing = TcOutgoing {
                move_token_out,
                token_info,
                opt_prev_move_token_in: None,
            };
            TokenChannel {
                direction: TcDirection::Outgoing(tc_outgoing),
                mutual_credits: ImHashMap::new(),
                active_currencies: ActiveCurrencies::new(),
            }
        } else {
            // We are the second sender
            let (move_token_in_full, token_info) =
                initial_move_token(remote_public_key, local_public_key);
            let move_token_in = create_hashed::<B>(&move_token_in_full, &token_info);

            let tc_incoming = TcIncoming { move_token_in };
            TokenChannel {
                direction: TcDirection::Incoming(tc_incoming),
                mutual_credits: ImHashMap::new(),
                active_currencies: ActiveCurrencies::new(),
            }
        }
    }

    pub fn get_mutual_credits(&self) -> &ImHashMap<Currency, MutualCredit> {
        &self.mutual_credits
    }

    pub fn get_active_currencies(&self) -> &ActiveCurrencies {
        &self.active_currencies
    }

    pub fn new_from_remote_reset(
        reset_move_token: &MoveToken<B>,
        remote_token_info: &TokenInfo,
    ) -> TokenChannel<B> {
        let mutual_credits: ImHashMap<Currency, MutualCredit> = remote_token_info
            .mc
            .balances
            .iter()
            .map(|currency_balance_info| {
                (
                    currency_balance_info.currency.clone(),
                    MutualCredit::new(
                        &remote_token_info.mc.remote_public_key,
                        &remote_token_info.mc.local_public_key,
                        &currency_balance_info.currency,
                        currency_balance_info
                            .balance_info
                            .balance
                            .checked_neg()
                            .unwrap(),
                    ),
                )
            })
            .collect();

        let active_currencies: ImHashSet<_> = mutual_credits.keys().cloned().collect();

        let tc_incoming = TcIncoming {
            move_token_in: create_hashed(&reset_move_token, remote_token_info),
        };

        TokenChannel {
            direction: TcDirection::Incoming(tc_incoming),
            mutual_credits,
            active_currencies: ActiveCurrencies {
                local: active_currencies.clone(),
                remote: active_currencies,
            },
        }
    }

    pub fn new_from_local_reset(
        reset_move_token: &MoveToken<B>,
        token_info: &TokenInfo,
        opt_last_incoming_move_token: Option<MoveTokenHashed>,
    ) -> TokenChannel<B> {
        let mutual_credits: ImHashMap<Currency, MutualCredit> = token_info
            .mc
            .balances
            .iter()
            .map(|currency_balance_info| {
                (
                    currency_balance_info.currency.clone(),
                    MutualCredit::new(
                        &token_info.mc.local_public_key,
                        &token_info.mc.remote_public_key,
                        &currency_balance_info.currency,
                        currency_balance_info.balance_info.balance,
                    ),
                )
            })
            .collect();

        let active_currencies: ImHashSet<_> = mutual_credits.keys().cloned().collect();
        let tc_outgoing = TcOutgoing {
            move_token_out: reset_move_token.clone(),
            token_info: token_info.clone(),
            opt_prev_move_token_in: opt_last_incoming_move_token,
        };

        TokenChannel {
            direction: TcDirection::Outgoing(tc_outgoing),
            mutual_credits,
            active_currencies: ActiveCurrencies {
                local: active_currencies.clone(),
                remote: active_currencies,
            },
        }
    }

    pub fn get_direction(&self) -> TcDirectionBorrow<'_, B> {
        match &self.direction {
            TcDirection::Incoming(tc_incoming) => TcDirectionBorrow::In(TcInBorrow {
                tc_incoming: &tc_incoming,
                mutual_credits: &self.mutual_credits,
                active_currencies: &self.active_currencies,
            }),
            TcDirection::Outgoing(tc_outgoing) => TcDirectionBorrow::Out(TcOutBorrow {
                tc_outgoing: &tc_outgoing,
                mutual_credits: &self.mutual_credits,
                active_currencies: &self.active_currencies,
            }),
        }
    }

    /// Get the last incoming move token
    /// If no such incoming move token exists (Maybe this is the beginning of the relationship),
    /// returns None.
    pub fn get_last_incoming_move_token_hashed(&self) -> Option<&MoveTokenHashed> {
        match &self.direction {
            TcDirection::Incoming(tc_incoming) => Some(&tc_incoming.move_token_in),
            TcDirection::Outgoing(tc_outgoing) => match &tc_outgoing.opt_prev_move_token_in {
                None => None,
                Some(prev_move_token_in) => Some(prev_move_token_in),
            },
        }
    }

    pub fn mutate(&mut self, d_mutation: &TcMutation<B>) {
        match d_mutation {
            TcMutation::McMutation((currency, mc_mutation)) => {
                let mutual_credit = self.mutual_credits.get_mut(currency).unwrap();
                mutual_credit.mutate(mc_mutation);
            }
            TcMutation::SetDirection(ref set_direction) => {
                self.direction = match set_direction {
                    SetDirection::Incoming(friend_move_token_hashed) => {
                        let tc_incoming = TcIncoming {
                            move_token_in: friend_move_token_hashed.clone(),
                        };
                        TcDirection::Incoming(tc_incoming)
                    }
                    SetDirection::Outgoing((friend_move_token, token_info)) => {
                        let tc_outgoing = TcOutgoing {
                            move_token_out: friend_move_token.clone(),
                            token_info: token_info.clone(),
                            opt_prev_move_token_in: self
                                .get_last_incoming_move_token_hashed()
                                .cloned(),
                        };
                        TcDirection::Outgoing(tc_outgoing)
                    }
                };
            }
            TcMutation::SetLocalActiveCurrencies(ref currencies) => {
                self.active_currencies.local = currencies.iter().cloned().collect();
            }
            TcMutation::SetRemoteActiveCurrencies(ref currencies) => {
                self.active_currencies.remote = currencies.iter().cloned().collect();
            }
            TcMutation::AddMutualCredit(ref currency) => {
                assert!(self.active_currencies.local.contains(currency));
                assert!(self.active_currencies.remote.contains(currency));

                let token_info = match &self.direction {
                    TcDirection::Incoming(tc_incoming) => {
                        tc_incoming.move_token_in.token_info.clone().flip()
                    }
                    TcDirection::Outgoing(tc_outgoing) => tc_outgoing.token_info.clone(),
                };

                let balance = 0;
                let new_mutual_credit = MutualCredit::new(
                    &token_info.mc.local_public_key,
                    &token_info.mc.remote_public_key,
                    currency,
                    balance,
                );
                let res = self
                    .mutual_credits
                    .insert(currency.clone(), new_mutual_credit);
                // Make sure that this currency was not already present:
                assert!(res.is_none());
            }
        }
    }

    pub fn get_inconsistency_counter(&self) -> u64 {
        match &self.direction {
            TcDirection::Incoming(tc_incoming) => {
                tc_incoming
                    .move_token_in
                    .token_info
                    .counters
                    .inconsistency_counter
            }
            TcDirection::Outgoing(tc_outgoing) => {
                tc_outgoing.token_info.counters.inconsistency_counter
            }
        }
    }

    pub fn get_outgoing(&self) -> Option<TcOutBorrow<'_, B>> {
        match self.get_direction() {
            TcDirectionBorrow::In(_) => None,
            TcDirectionBorrow::Out(tc_out_borrow) => Some(tc_out_borrow),
        }
    }

    pub fn get_incoming(&self) -> Option<TcInBorrow<'_>> {
        match self.get_direction() {
            TcDirectionBorrow::In(tc_in_borrow) => Some(tc_in_borrow),
            TcDirectionBorrow::Out(_) => None,
        }
    }

    pub fn simulate_receive_move_token(
        &self,
        new_move_token: MoveToken<B>,
        remote_max_debts: &ImHashMap<Currency, u128>,
    ) -> Result<ReceiveMoveTokenOutput<B>, ReceiveMoveTokenError> {
        match &self.get_direction() {
            TcDirectionBorrow::In(tc_in_borrow) => tc_in_borrow.handle_incoming(new_move_token),
            TcDirectionBorrow::Out(tc_out_borrow) => {
                tc_out_borrow.handle_incoming(new_move_token, remote_max_debts)
            }
        }
    }
}
*/

/*
// TODO: Reimplement later
impl<'a> TcInBorrow<'a> {
    /// Create a full TokenChannel (Incoming direction)
    fn create_token_channel<B>(&self) -> TokenChannel<B> {
        TokenChannel {
            direction: TcDirection::Incoming(self.tc_incoming.clone()),
            mutual_credits: self.mutual_credits.clone(),
            active_currencies: self.active_currencies.clone(),
        }
    }

    /// Handle an incoming move token during Incoming direction:
    fn handle_incoming<B>(
        &self,
        new_move_token: MoveToken<B>,
    ) -> Result<ReceiveMoveTokenOutput<B>, ReceiveMoveTokenError>
    where
        B: CanonicalSerialize + Clone,
    {
        // We compare the whole move token message and not just the signature (new_token)
        // because we don't check the signature in this flow.
        if self.tc_incoming.move_token_in
            == create_hashed(&new_move_token, &self.tc_incoming.move_token_in.token_info)
        {
            // Duplicate
            Ok(ReceiveMoveTokenOutput::Duplicate)
        } else {
            // Inconsistency
            Err(ReceiveMoveTokenError::ChainInconsistency)
        }
    }

    pub fn simulate_send_move_token<B>(
        &self,
        currencies_operations: Vec<CurrencyOperations>,
        opt_local_relays: Option<Vec<RelayAddress<B>>>,
        opt_active_currencies: Option<Vec<Currency>>,
        rand_nonce: RandValue,
    ) -> Result<SendMoveTokenOutput<B>, SendMoveTokenError>
    where
        B: CanonicalSerialize + Clone,
    {
        // We create a clone `token_channel` on which we are going to apply all the mutations.
        // Eventually this cloned TokenChannel is discarded, and we only output the applied mutations.
        let mut token_channel = self.create_token_channel();
        let tc_in_borrow = token_channel.get_incoming().unwrap();

        let mut tc_mutations = Vec::new();

        if let Some(active_currencies) = opt_active_currencies.as_ref() {
            for mutual_credit_currency in tc_in_borrow.mutual_credits.keys() {
                if !active_currencies.contains(&mutual_credit_currency) {
                    return Err(SendMoveTokenError::CanNotRemoveCurrencyInUse);
                }
            }

            let mutation = TcMutation::SetLocalActiveCurrencies(active_currencies.clone());
            token_channel.mutate(&mutation);
            tc_mutations.push(mutation);

            let tc_in_borrow = token_channel.get_incoming().unwrap();
            let active_currencies = &tc_in_borrow.active_currencies;

            // Find the new currencies we need to initialize.
            // Calculate:
            // (local ^ remote) \ mutual_credit_currencies:
            let intersection = active_currencies
                .remote
                .clone()
                .intersection(active_currencies.local.clone());

            let mutual_credit_currencies: ImHashSet<_> =
                tc_in_borrow.mutual_credits.keys().cloned().collect();

            let new_currencies = intersection.relative_complement(mutual_credit_currencies);

            for new_currency in new_currencies {
                let mutation = TcMutation::AddMutualCredit(new_currency.clone());
                token_channel.mutate(&mutation);
                tc_mutations.push(mutation);
            }
        }

        // Update mutual credits:
        for currency_operations in &currencies_operations {
            let tc_in_borrow = token_channel.get_incoming().unwrap();
            let mutual_credit = tc_in_borrow
                .mutual_credits
                .get(&currency_operations.currency)
                .unwrap()
                .clone();

            let mut outgoing_mc = OutgoingMc::new(&mutual_credit);
            for op in &currency_operations.operations {
                let mc_mutations = outgoing_mc.queue_operation(op).unwrap();
                for mc_mutation in mc_mutations {
                    let mutation = TcMutation::McMutation((
                        currency_operations.currency.clone(),
                        mc_mutation.clone(),
                    ));
                    token_channel.mutate(&mutation);
                    tc_mutations.push(mutation);
                }
            }
        }

        let tc_in_borrow = token_channel.get_incoming().unwrap();

        let mut balances: Vec<_> = tc_in_borrow
            .mutual_credits
            .iter()
            .map(|(currency, mutual_credit)| CurrencyBalanceInfo {
                currency: currency.clone(),
                balance_info: BalanceInfo {
                    balance: mutual_credit.state().balance.balance,
                    local_pending_debt: mutual_credit.state().balance.local_pending_debt,
                    remote_pending_debt: mutual_credit.state().balance.remote_pending_debt,
                },
            })
            .collect();

        // Canonicalize balances:
        balances.sort_by(|cbi1, cbi2| cbi1.currency.cmp(&cbi2.currency));

        let tc_in_borrow = token_channel.get_incoming().unwrap();
        let cur_token_info = &tc_in_borrow.tc_incoming.move_token_in.token_info;

        let token_info = TokenInfo {
            mc: McInfo {
                local_public_key: cur_token_info.mc.remote_public_key.clone(),
                remote_public_key: cur_token_info.mc.local_public_key.clone(),
                balances,
            },
            counters: CountersInfo {
                move_token_counter: cur_token_info.counters.move_token_counter.wrapping_add(1),
                inconsistency_counter: cur_token_info.counters.inconsistency_counter,
            },
        };

        let unsigned_move_token = create_unsigned_move_token(
            currencies_operations,
            opt_local_relays,
            opt_active_currencies,
            &token_info,
            tc_in_borrow.tc_incoming.move_token_in.new_token.clone(),
            rand_nonce,
        );

        Ok(SendMoveTokenOutput {
            unsigned_move_token,
            mutations: tc_mutations,
            token_info,
        })
    }

    pub fn create_outgoing_mc(&self, currency: &Currency) -> Option<OutgoingMc> {
        Some(OutgoingMc::new(self.mutual_credits.get(currency)?))
    }
}

impl<'a, B> TcOutBorrow<'a, B>
where
    B: Clone + CanonicalSerialize,
{
    /// Handle an incoming move token during Outgoing direction:
    fn handle_incoming(
        &self,
        new_move_token: MoveToken<B>,
        remote_max_debts: &ImHashMap<Currency, u128>,
    ) -> Result<ReceiveMoveTokenOutput<B>, ReceiveMoveTokenError> {
        if new_move_token.old_token == self.tc_outgoing.move_token_out.new_token {
            Ok(ReceiveMoveTokenOutput::Received(
                self.handle_incoming_token_match(new_move_token, remote_max_debts)?,
            ))
        // self.outgoing_to_incoming(friend_move_token, new_move_token)
        } else if self.tc_outgoing.move_token_out.old_token == new_move_token.new_token {
            // We should retransmit our move token message to the remote side.
            Ok(ReceiveMoveTokenOutput::RetransmitOutgoing(
                self.tc_outgoing.move_token_out.clone(),
            ))
        } else {
            Err(ReceiveMoveTokenError::ChainInconsistency)
        }
    }

    /// Create a full TokenChannel (Outgoing direction)
    fn create_token_channel(&self) -> TokenChannel<B> {
        TokenChannel {
            direction: TcDirection::Outgoing(self.tc_outgoing.clone()),
            mutual_credits: self.mutual_credits.clone(),
            active_currencies: self.active_currencies.clone(),
        }
    }

    /// Get the current outgoing move token
    pub fn create_outgoing_move_token(&self) -> MoveToken<B> {
        self.tc_outgoing.move_token_out.clone()
    }
}
*/

// TODO: Restore tests
/*

#[cfg(test)]
mod tests {
    use super::*;

    use proto::crypto::PrivateKey;
    // use proto::funder::messages::FriendTcOp;

    use crypto::identity::Identity;
    use crypto::identity::SoftwareEd25519Identity;
    use crypto::rand::RandGen;
    use crypto::test_utils::DummyRandom;

    use signature::signature_buff::move_token_signature_buff;

    /// A helper function to sign an UnsignedMoveToken using an identity:
    fn dummy_sign_move_token<B, I>(
        unsigned_move_token: UnsignedMoveToken<B>,
        identity: &I,
    ) -> MoveToken<B>
    where
        B: CanonicalSerialize + Clone,
        I: Identity,
    {
        let signature_buff = move_token_signature_buff(unsigned_move_token.clone());

        MoveToken {
            old_token: unsigned_move_token.old_token,
            currencies_operations: unsigned_move_token.currencies_operations,
            opt_local_relays: unsigned_move_token.opt_local_relays,
            opt_active_currencies: unsigned_move_token.opt_active_currencies,
            info_hash: unsigned_move_token.info_hash,
            rand_nonce: unsigned_move_token.rand_nonce,
            new_token: identity.sign(&signature_buff),
        }
    }

    #[test]
    fn test_initial_direction() {
        let pk_a = PublicKey::from(&[0xaa; PublicKey::len()]);
        let pk_b = PublicKey::from(&[0xbb; PublicKey::len()]);
        let token_channel_a_b = TokenChannel::<u32>::new(&pk_a, &pk_b);
        let token_channel_b_a = TokenChannel::<u32>::new(&pk_b, &pk_a);

        // Only one of those token channels is outgoing:
        let is_a_b_outgoing = token_channel_a_b.get_outgoing().is_some();
        let is_b_a_outgoing = token_channel_b_a.get_outgoing().is_some();
        assert!(is_a_b_outgoing ^ is_b_a_outgoing);

        let (out_tc, in_tc) = if is_a_b_outgoing {
            (token_channel_a_b, token_channel_b_a)
        } else {
            (token_channel_b_a, token_channel_a_b)
        };

        let out_hashed = match &out_tc.direction {
            TcDirection::Incoming(_) => unreachable!(),
            TcDirection::Outgoing(outgoing) => {
                create_hashed(&outgoing.move_token_out, &outgoing.token_info)
            }
        };

        let in_hashed = match &in_tc.direction {
            TcDirection::Outgoing(_) => unreachable!(),
            TcDirection::Incoming(incoming) => &incoming.move_token_in,
        };

        assert_eq!(&out_hashed, in_hashed);

        // assert_eq!(create_hashed(&out_tc.move_token_out), in_tc.move_token_in);
        // assert_eq!(out_tc.get_cur_move_token_hashed(), in_tc.get_cur_move_token_hashed());
        let tc_outgoing = match out_tc.get_direction() {
            TcDirectionBorrow::Out(tc_out_borrow) => tc_out_borrow.tc_outgoing,
            TcDirectionBorrow::In(_) => unreachable!(),
        };
        assert!(tc_outgoing.opt_prev_move_token_in.is_none());
    }

    /// Sort the two identity client.
    /// The result will be a pair where the first is initially configured to have outgoing message,
    /// and the second is initially configured to have incoming message.
    fn sort_sides<I>(identity1: I, identity2: I) -> (I, I)
    where
        I: Identity,
    {
        let pk1 = identity1.get_public_key();
        let pk2 = identity2.get_public_key();
        let token_channel12 = TokenChannel::<u32>::new(&pk1, &pk2); // (local, remote)
        if token_channel12.get_outgoing().is_some() {
            (identity1, identity2)
        } else {
            (identity2, identity1)
        }
    }
    /// Before: tc1: outgoing, tc2: incoming
    /// Send AddCurrency: tc2 -> tc1
    /// After: tc1: incoming, tc2: outgoing
    fn add_currencies<I>(
        _identity1: &I,
        identity2: &I,
        tc1: &mut TokenChannel<u32>,
        tc2: &mut TokenChannel<u32>,
        currencies: &[Currency],
    ) where
        I: Identity,
    {
        assert!(tc1.get_outgoing().is_some());
        assert!(!tc2.get_outgoing().is_some());

        let tc2_in_borrow = tc2.get_incoming().unwrap();
        let currencies_operations = vec![];

        let rand_nonce = RandValue::from(&[7; RandValue::len()]);
        let opt_local_relays = None;
        let opt_active_currencies = Some(currencies.to_vec());

        let SendMoveTokenOutput {
            unsigned_move_token,
            mutations,
            token_info,
        } = tc2_in_borrow
            .simulate_send_move_token(
                currencies_operations,
                opt_local_relays,
                opt_active_currencies,
                rand_nonce,
            )
            .unwrap();

        for tc_mutation in mutations {
            tc2.mutate(&tc_mutation);
        }

        // This is the only mutation we can not produce from inside TokenChannel, because it
        // requires a signature:
        let friend_move_token = dummy_sign_move_token(unsigned_move_token, identity2);
        let tc_mutation = TcMutation::SetDirection(SetDirection::Outgoing((
            friend_move_token.clone(),
            token_info.clone(),
        )));
        tc2.mutate(&tc_mutation);

        assert!(tc2.get_outgoing().is_some());

        let receive_move_token_output = tc1
            .simulate_receive_move_token(friend_move_token.clone(), &ImHashMap::new())
            .unwrap();

        let move_token_received = match receive_move_token_output {
            ReceiveMoveTokenOutput::Received(move_token_received) => move_token_received,
            _ => unreachable!(),
        };

        assert!(move_token_received.currencies.is_empty());

        // let mut seen_mc_mutation = false;
        // let mut seen_set_direction = false;

        for tc_mutation in &move_token_received.mutations {
            tc1.mutate(tc_mutation);
        }

        assert!(!tc1.get_outgoing().is_some());
        match &tc1.direction {
            TcDirection::Outgoing(_) => unreachable!(),
            TcDirection::Incoming(tc_incoming) => {
                assert_eq!(
                    tc_incoming.move_token_in,
                    create_hashed(&friend_move_token, &token_info)
                );
            }
        };

        for currency in currencies {
            assert!(tc2.active_currencies.local.contains(currency));
            assert!(tc1.active_currencies.remote.contains(currency));
        }
    }

    /*
    /// Before: tc1: outgoing, tc2: incoming
    /// Send SetRemoteMaxDebt: tc2 -> tc1
    /// After: tc1: incoming, tc2: outgoing
    fn set_remote_max_debt21<I>(
        _identity1: &I,
        identity2: &I,
        tc1: &mut TokenChannel<u32>,
        tc2: &mut TokenChannel<u32>,
        currency: &Currency,
    ) where
        I: Identity,
    {
        assert!(tc1.get_outgoing().is_some());
        assert!(tc2.get_incoming().is_some());

        let tc2_in_borrow = tc2.get_incoming().unwrap();
        // let mut outgoing_mc = tc2_in_borrow.create_outgoing_mc(&currency).unwrap();
        //
        // let friend_tc_op = FriendTcOp::SetRemoteMaxDebt(100);
        //
        // let mc_mutations = outgoing_mc.queue_operation(&friend_tc_op).unwrap();
        let currency_operations = CurrencyOperations {
            currency: currency.clone(),
            operations: vec![friend_tc_op],
        };
        let currencies_operations = vec![currency_operations];

        let rand_nonce = RandValue::from(&[5; RandValue::len()]);
        let opt_local_relays = None;
        let opt_active_currencies = None;

        let SendMoveTokenOutput {
            unsigned_move_token,
            mutations,
            token_info,
        } = tc2_in_borrow
            .simulate_send_move_token(
                currencies_operations,
                opt_local_relays,
                opt_active_currencies,
                rand_nonce,
            )
            .unwrap();

        for tc_mutation in mutations {
            tc2.mutate(&tc_mutation);
        }

        let friend_move_token = dummy_sign_move_token(unsigned_move_token, identity2);
        let tc_mutation = TcMutation::SetDirection(SetDirection::Outgoing((
            friend_move_token.clone(),
            token_info.clone(),
        )));
        tc2.mutate(&tc_mutation);

        assert!(tc2.get_outgoing().is_some());

        let receive_move_token_output = tc1
            .simulate_receive_move_token(friend_move_token.clone())
            .unwrap();

        let move_token_received = match receive_move_token_output {
            ReceiveMoveTokenOutput::Received(move_token_received) => move_token_received,
            _ => unreachable!(),
        };

        assert!(move_token_received.currencies[0]
            .incoming_messages
            .is_empty());
        assert_eq!(move_token_received.mutations.len(), 2);

        let mut seen_mc_mutation = false;
        let mut seen_set_direction = false;

        for i in 0..2 {
            match &move_token_received.mutations[i] {
                TcMutation::McMutation(mc_mutation) => {
                    seen_mc_mutation = true;
                    assert_eq!(
                        mc_mutation,
                        &(currency.clone(), McMutation::SetLocalMaxDebt(100))
                    );
                }
                TcMutation::SetDirection(set_direction) => {
                    seen_set_direction = true;
                    match set_direction {
                        SetDirection::Incoming(incoming_friend_move_token) => assert_eq!(
                            &create_hashed(&friend_move_token, &token_info),
                            incoming_friend_move_token
                        ),
                        _ => unreachable!(),
                    }
                }
                _ => unreachable!(),
            }
        }
        assert!(seen_mc_mutation && seen_set_direction);

        for tc_mutation in &move_token_received.mutations {
            tc1.mutate(tc_mutation);
        }

        assert!(!tc1.get_outgoing().is_some());
        match &tc1.direction {
            TcDirection::Outgoing(_) => unreachable!(),
            TcDirection::Incoming(tc_incoming) => {
                assert_eq!(
                    tc_incoming.move_token_in,
                    create_hashed(&friend_move_token, &token_info)
                );
            }
        };

        assert_eq!(
            tc1.mutual_credits
                .get(&currency)
                .unwrap()
                .state()
                .balance
                .local_max_debt,
            100
        );
    }
    */

    /// This tests sends a SetRemoteMaxDebt(100) in both ways.
    #[test]
    fn test_simulate_receive_move_token_basic() {
        let currency = Currency::try_from("FST".to_owned()).unwrap();

        let mut rng1 = DummyRandom::new(&[1u8]);
        let pkcs8 = PrivateKey::rand_gen(&mut rng1);
        let identity1 = SoftwareEd25519Identity::from_private_key(&pkcs8).unwrap();

        let mut rng2 = DummyRandom::new(&[2u8]);
        let pkcs8 = PrivateKey::rand_gen(&mut rng2);
        let identity2 = SoftwareEd25519Identity::from_private_key(&pkcs8).unwrap();

        let (identity1, identity2) = sort_sides(identity1, identity2);

        let pk1 = identity1.get_public_key();
        let pk2 = identity2.get_public_key();
        let mut tc1 = TokenChannel::<u32>::new(&pk1, &pk2); // (local, remote)
        let mut tc2 = TokenChannel::<u32>::new(&pk2, &pk1); // (local, remote)

        // Current state:  tc1 --> tc2
        // tc1: outgoing
        // tc2: incoming
        add_currencies(
            &identity1,
            &identity2,
            &mut tc1,
            &mut tc2,
            &[currency.clone()],
        );

        // Current state:  tc2 --> tc1
        // tc1: incoming
        // tc2: outgoing
        add_currencies(
            &identity2,
            &identity1,
            &mut tc2,
            &mut tc1,
            &[currency.clone()],
        );

        // Current state:  tc1 --> tc2
        // tc1: outgoing
        // tc2: incoming
        // set_remote_max_debt21(&identity1, &identity2, &mut tc1, &mut tc2, &currency);

        // Current state:  tc2 --> tc1
        // tc1: incoming
        // tc2: outgoing
        // set_remote_max_debt21(&identity2, &identity1, &mut tc2, &mut tc1, &currency);
    }

    // TODO: Add more tests.
    // - Test behaviour of Duplicate, ChainInconsistency
}
*/
