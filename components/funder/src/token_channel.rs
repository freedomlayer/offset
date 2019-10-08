use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::convert::TryFrom;

use im::hashmap::HashMap as ImHashMap;
use im::hashset::HashSet as ImHashSet;

use signature::canonical::CanonicalSerialize;

use crypto::hash::sha_512_256;
use crypto::identity::compare_public_key;

use proto::crypto::{PublicKey, RandValue, Signature};

use proto::app_server::messages::RelayAddress;
use proto::funder::messages::{
    CountersInfo, Currency, CurrencyOperations, McInfo, MoveToken, TokenInfo,
};
use signature::signature_buff::hash_token_info;
use signature::verify::verify_move_token;

use crate::mutual_credit::incoming::{
    process_operations_list, IncomingMessage, ProcessOperationOutput, ProcessTransListError,
};
use crate::mutual_credit::outgoing::OutgoingMc;
use crate::mutual_credit::types::{McMutation, MutualCredit};

use crate::types::{create_hashed, create_unsigned_move_token, MoveTokenHashed, UnsignedMoveToken};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SetDirection<B> {
    Incoming(MoveTokenHashed),
    Outgoing((MoveToken<B>, TokenInfo)),
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TcMutation<B> {
    McMutation((Currency, McMutation)),
    AddLocalActiveCurrency(Currency),
    AddRemoteActiveCurrency(Currency),
    AddMutualCredit(Currency),
    SetDirection(SetDirection<B>),
}

/// The currencies set to be active by two sides of the token channel.
/// Only currencies that are active on both sides (Intersection) can be used for trading.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
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
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct TcOutgoing<B> {
    pub move_token_out: MoveToken<B>,
    pub token_info: TokenInfo,
    pub opt_prev_move_token_in: Option<MoveTokenHashed>,
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct TcIncoming {
    pub move_token_in: MoveTokenHashed,
}

#[allow(clippy::large_enum_variant)]
#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Eq)]
pub enum TcDirection<B> {
    Incoming(TcIncoming),
    Outgoing(TcOutgoing<B>),
}

#[derive(Clone, Debug)]
pub struct TcInBorrow<'a> {
    tc_incoming: &'a TcIncoming,
    mutual_credits: &'a ImHashMap<Currency, MutualCredit>,
    active_currencies: &'a ActiveCurrencies,
}

#[derive(Clone, Debug)]
pub struct TcOutBorrow<'a, B> {
    tc_outgoing: &'a TcOutgoing<B>,
    mutual_credits: &'a ImHashMap<Currency, MutualCredit>,
    active_currencies: &'a ActiveCurrencies,
}

#[derive(Clone, Debug)]
pub enum TcDirectionBorrow<'a, B> {
    In(TcInBorrow<'a>),
    Out(TcOutBorrow<'a, B>),
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct TokenChannel<B> {
    direction: TcDirection<B>,
    mutual_credits: ImHashMap<Currency, MutualCredit>,
    active_currencies: ActiveCurrencies,
}

#[derive(Debug)]
pub enum ReceiveMoveTokenError {
    ChainInconsistency,
    InvalidTransaction(ProcessTransListError),
    InvalidSignature,
    InvalidStatedBalance,
    InvalidInconsistencyCounter,
    MoveTokenCounterOverflow,
    InvalidMoveTokenCounter,
    TooManyOperations,
    InvalidCurrency,
    InvalidAddActiveCurrencies,
}

#[derive(Debug)]
pub struct MoveTokenReceivedCurrency<B> {
    pub currency: Currency,
    pub incoming_messages: Vec<IncomingMessage>,
    pub remote_requests_closed: bool,
    pub opt_local_relays: Option<Vec<RelayAddress<B>>>,
}

#[derive(Debug)]
pub struct MoveTokenReceived<B> {
    pub mutations: Vec<TcMutation<B>>,
    pub currencies: Vec<MoveTokenReceivedCurrency<B>>,
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
pub enum ReceiveMoveTokenOutput<B> {
    Duplicate,
    RetransmitOutgoing(MoveToken<B>),
    Received(MoveTokenReceived<B>),
    // In case of a reset, all the local pending requests will be canceled.
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
        opt_local_relays: None,
        opt_add_active_currencies: None,
        info_hash: hash_token_info(&token_info),
        rand_nonce: rand_nonce_from_public_key(&high_public_key),
        new_token: token_from_public_key(&high_public_key),
    };

    (move_token, token_info)
}

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

    pub fn get_remote_max_debt(&self, currency: &Currency) -> u128 {
        self.mutual_credits
            .get(currency)
            .unwrap()
            .state()
            .balance
            .remote_max_debt
    }

    pub fn get_direction<'a>(&'a self) -> TcDirectionBorrow<'a, B> {
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
            TcMutation::AddLocalActiveCurrency(ref currency) => {
                self.active_currencies.local.insert(currency.clone());
            }
            TcMutation::AddRemoteActiveCurrency(ref currency) => {
                self.active_currencies.remote.insert(currency.clone());
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

    /*
    pub fn get_move_token_counter(&self) -> u128 {
        match &self.direction {
            TcDirection::Incoming(tc_incoming) => {
                tc_incoming.move_token_in.token_info.move_token_counter
            }
            TcDirection::Outgoing(tc_outgoing) => tc_outgoing.token_info.move_token_counter,
        }
    }
    */

    pub fn is_outgoing(&self) -> bool {
        match self.direction {
            TcDirection::Incoming(_) => false,
            TcDirection::Outgoing(_) => true,
        }
    }

    pub fn simulate_receive_move_token(
        &self,
        new_move_token: MoveToken<B>,
    ) -> Result<ReceiveMoveTokenOutput<B>, ReceiveMoveTokenError> {
        match &self.get_direction() {
            TcDirectionBorrow::In(tc_in_borrow) => tc_in_borrow.handle_incoming(new_move_token),
            TcDirectionBorrow::Out(tc_out_borrow) => tc_out_borrow.handle_incoming(new_move_token),
        }
    }
}

impl<'a> TcInBorrow<'a> {
    /// Handle an incoming move token during Incoming direction:
    fn handle_incoming<B>(
        &self,
        new_move_token: MoveToken<B>,
    ) -> Result<ReceiveMoveTokenOutput<B>, ReceiveMoveTokenError>
    where
        B: CanonicalSerialize,
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

    pub fn create_unsigned_move_token<B>(
        &self,
        currencies_operations: Vec<CurrencyOperations>,
        opt_local_relays: Option<Vec<RelayAddress<B>>>,
        opt_add_active_currencies: Option<Vec<Currency>>,
        rand_nonce: RandValue,
    ) -> (UnsignedMoveToken<B>, TokenInfo) {
        let mut token_info = self.tc_incoming.move_token_in.token_info.clone().flip();
        token_info.counters.move_token_counter =
            token_info.counters.move_token_counter.wrapping_add(1);

        let unsigned_move_token = create_unsigned_move_token(
            currencies_operations,
            opt_local_relays,
            opt_add_active_currencies,
            &token_info,
            self.tc_incoming.move_token_in.new_token.clone(),
            rand_nonce,
        );

        (unsigned_move_token, token_info)
    }

    pub fn begin_outgoing_move_token(&self, currency: &Currency) -> Option<OutgoingMc> {
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
    ) -> Result<ReceiveMoveTokenOutput<B>, ReceiveMoveTokenError> {
        if new_move_token.old_token == self.tc_outgoing.move_token_out.new_token {
            Ok(ReceiveMoveTokenOutput::Received(
                self.handle_incoming_token_match(new_move_token)?,
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

    fn handle_incoming_token_match(
        &self,
        new_move_token: MoveToken<B>,
    ) -> Result<MoveTokenReceived<B>, ReceiveMoveTokenError> {
        // Verify signature:
        // Note that we only verify the signature here, and not at the Incoming part.
        // This allows the genesis move token to occur smoothly, even though its signature
        // is not correct.
        let remote_public_key = &self.tc_outgoing.token_info.mc.remote_public_key;
        if !verify_move_token(&new_move_token, remote_public_key) {
            return Err(ReceiveMoveTokenError::InvalidSignature);
        }

        // Aggregate results for every currency:
        let mut move_token_received = MoveTokenReceived {
            mutations: Vec::new(),
            currencies: Vec::new(),
        };

        let mutual_credits = self.mutual_credits.clone();

        /*
        if let Some(ref add_active_currencies) = new_move_token.opt_add_active_currencies {
            for currency in add_active_currencies {
                if self.active_currencies.remote.contains(currency) {
                    return Err(ReceiveMoveTokenError::InvalidAddActiveCurrencies);
                }
                if self.active_currencies.local.contains(currency) {
                    move_token_received.mutations.push(TcMutation::SetDirection(
                        SetDirection::Incoming(create_hashed(&new_move_token, &token_info)),
                    ));

                    // TODO
                }
            }
        }
        */

        // TODO: We might need to create a new mutual_credit here.
        // according to new_move_token.opt_add_active_currencies
        assert!(false);

        for currency_operations in &new_move_token.currencies_operations {
            let mut mutual_credit = self
                .mutual_credits
                .get(&currency_operations.currency)
                .ok_or(ReceiveMoveTokenError::InvalidCurrency)?
                .clone();

            let outputs =
                process_operations_list(&mut mutual_credit, currency_operations.operations.clone())
                    .map_err(ReceiveMoveTokenError::InvalidTransaction)?;

            let mut token_info = self.tc_outgoing.token_info.clone().flip();
            token_info.counters.move_token_counter = token_info
                .counters
                .move_token_counter
                .checked_add(1)
                .ok_or(ReceiveMoveTokenError::MoveTokenCounterOverflow)?;

            let initial_remote_requests = mutual_credit.state().requests_status.remote.is_open();

            let mut incoming_messages = Vec::new();

            // We apply mutations on this token channel, to verify stated balance values
            let mut check_mutual_credit = mutual_credit.clone();

            let mut final_remote_requests: bool = initial_remote_requests;
            for output in outputs {
                let ProcessOperationOutput {
                    incoming_message,
                    mc_mutations,
                } = output;

                if let Some(funds) = incoming_message {
                    incoming_messages.push(funds);
                }
                for mc_mutation in mc_mutations {
                    check_mutual_credit.mutate(&mc_mutation);
                    if let McMutation::SetRemoteRequestsStatus(requests_status) = &mc_mutation {
                        final_remote_requests = requests_status.is_open();
                    }
                    move_token_received.mutations.push(TcMutation::McMutation((
                        currency_operations.currency.clone(),
                        mc_mutation,
                    )));
                }
            }

            // Verify stated balances:
            let info_hash = hash_token_info(&token_info);
            if new_move_token.info_hash != info_hash {
                return Err(ReceiveMoveTokenError::InvalidStatedBalance);
            }

            move_token_received
                .mutations
                .push(TcMutation::SetDirection(SetDirection::Incoming(
                    create_hashed(&new_move_token, &token_info),
                )));

            let move_token_received_currency = MoveTokenReceivedCurrency {
                currency: currency_operations.currency.clone(),
                incoming_messages,
                // Were the remote requests initially open and now it is closed?
                remote_requests_closed: final_remote_requests && !initial_remote_requests,
                opt_local_relays: new_move_token.opt_local_relays.clone(),
            };

            move_token_received
                .currencies
                .push(move_token_received_currency);
        }

        Ok(move_token_received)
    }

    /// Get the current outgoing move token
    pub fn create_outgoing_move_token(&self) -> MoveToken<B> {
        self.tc_outgoing.move_token_out.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crypto::identity::Identity;
    use crypto::identity::{generate_private_key, SoftwareEd25519Identity};
    use crypto::test_utils::DummyRandom;

    use signature::signature_buff::move_token_signature_buff;

    /// A helper function to sign an UnsignedMoveToken using an identity:
    fn dummy_sign_move_token<B, I>(
        unsigned_move_token: UnsignedMoveToken<B>,
        identity: &I,
    ) -> MoveToken<B>
    where
        B: CanonicalSerialize,
        I: Identity,
    {
        let signature_buff = move_token_signature_buff(&unsigned_move_token);

        MoveToken {
            operations: unsigned_move_token.operations,
            opt_local_relays: unsigned_move_token.opt_local_relays,
            info_hash: unsigned_move_token.info_hash,
            old_token: unsigned_move_token.old_token,
            rand_nonce: unsigned_move_token.rand_nonce,
            new_token: identity.sign(&signature_buff),
        }
    }

    #[test]
    fn test_initial_direction() {
        let pk_a = PublicKey::from(&[0xaa; PublicKey::len()]);
        let pk_b = PublicKey::from(&[0xbb; PublicKey::len()]);
        let token_channel_a_b = TokenChannel::<u32>::new(&pk_a, &pk_b, 0i128);
        let token_channel_b_a = TokenChannel::<u32>::new(&pk_b, &pk_a, 0i128);

        // Only one of those token channels is outgoing:
        let is_a_b_outgoing = token_channel_a_b.is_outgoing();
        let is_b_a_outgoing = token_channel_b_a.is_outgoing();
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
            TcDirection::Outgoing(tc_outgoing) => tc_outgoing,
            TcDirection::Incoming(_) => unreachable!(),
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
        let token_channel12 = TokenChannel::<u32>::new(&pk1, &pk2, 0i128); // (local, remote)
        if token_channel12.is_outgoing() {
            (identity1, identity2)
        } else {
            (identity2, identity1)
        }
    }

    /// Before: tc1: outgoing, tc2: incoming
    /// Send SetRemoteMaxDebt: tc2 -> tc1
    /// After: tc1: incoming, tc2: outgoing
    fn set_remote_max_debt21<I>(
        _identity1: &I,
        identity2: &I,
        tc1: &mut TokenChannel<u32>,
        tc2: &mut TokenChannel<u32>,
    ) where
        I: Identity,
    {
        assert!(tc1.is_outgoing());
        assert!(!tc2.is_outgoing());

        let tc2_incoming = match tc2.get_direction() {
            TcDirection::Incoming(tc2_incoming) => tc2_incoming,
            TcDirection::Outgoing(_) => unreachable!(),
        };
        let mut outgoing_mc = tc2_incoming.begin_outgoing_move_token();
        let friend_tc_op = FriendTcOp::SetRemoteMaxDebt(100);
        let mc_mutations = outgoing_mc.queue_operation(&friend_tc_op).unwrap();
        let operations = vec![friend_tc_op];

        let rand_nonce = RandValue::from(&[5; RandValue::len()]);
        let opt_local_relays = None;
        let (unsigned_move_token, token_info) =
            tc2_incoming.create_unsigned_move_token(operations, opt_local_relays, rand_nonce);

        let friend_move_token = dummy_sign_move_token(unsigned_move_token, identity2);

        for mc_mutation in mc_mutations {
            tc2.mutate(&TcMutation::McMutation(mc_mutation));
        }
        let tc_mutation = TcMutation::SetDirection(SetDirection::Outgoing((
            friend_move_token.clone(),
            token_info.clone(),
        )));
        tc2.mutate(&tc_mutation);

        assert!(tc2.is_outgoing());

        let receive_move_token_output = tc1
            .simulate_receive_move_token(friend_move_token.clone())
            .unwrap();

        let move_token_received = match receive_move_token_output {
            ReceiveMoveTokenOutput::Received(move_token_received) => move_token_received,
            _ => unreachable!(),
        };

        assert!(move_token_received.incoming_messages.is_empty());
        assert_eq!(move_token_received.mutations.len(), 2);

        let mut seen_mc_mutation = false;
        let mut seen_set_direction = false;

        for i in 0..2 {
            match &move_token_received.mutations[i] {
                TcMutation::McMutation(mc_mutation) => {
                    seen_mc_mutation = true;
                    assert_eq!(mc_mutation, &McMutation::SetLocalMaxDebt(100));
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
            }
        }
        assert!(seen_mc_mutation && seen_set_direction);

        for tc_mutation in &move_token_received.mutations {
            tc1.mutate(tc_mutation);
        }

        assert!(!tc1.is_outgoing());
        match &tc1.direction {
            TcDirection::Outgoing(_) => unreachable!(),
            TcDirection::Incoming(tc_incoming) => {
                assert_eq!(
                    tc_incoming.move_token_in,
                    create_hashed(&friend_move_token, &token_info)
                );
            }
        };
        // assert_eq!(&tc1.get_cur_move_token_hashed(), &create_hashed(&friend_move_token));
        assert_eq!(tc1.get_mutual_credit().state().balance.local_max_debt, 100);
    }

    /// This tests sends a SetRemoteMaxDebt(100) in both ways.
    #[test]
    fn test_simulate_receive_move_token_basic() {
        let rng1 = DummyRandom::new(&[1u8]);
        let pkcs8 = generate_private_key(&rng1);
        let identity1 = SoftwareEd25519Identity::from_private_key(&pkcs8).unwrap();

        let rng2 = DummyRandom::new(&[2u8]);
        let pkcs8 = generate_private_key(&rng2);
        let identity2 = SoftwareEd25519Identity::from_private_key(&pkcs8).unwrap();

        let (identity1, identity2) = sort_sides(identity1, identity2);

        let pk1 = identity1.get_public_key();
        let pk2 = identity2.get_public_key();
        let mut tc1 = TokenChannel::new(&pk1, &pk2, 0i128); // (local, remote)
        let mut tc2 = TokenChannel::new(&pk2, &pk1, 0i128); // (local, remote)

        // Current state:  tc1 --> tc2
        // tc1: outgoing
        // tc2: incoming
        set_remote_max_debt21(&identity1, &identity2, &mut tc1, &mut tc2);

        // Current state:  tc2 --> tc1
        // tc1: incoming
        // tc2: outgoing
        set_remote_max_debt21(&identity2, &identity1, &mut tc2, &mut tc1);
    }

    // TODO: Add more tests.
    // - Test behaviour of Duplicate, ChainInconsistency
}
