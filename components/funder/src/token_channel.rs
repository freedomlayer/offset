use std::cmp::Ordering;
use std::convert::TryFrom;
use std::iter::IntoIterator;

use derive_more::From;

use futures::channel::oneshot;
use futures::{Stream, StreamExt, TryStreamExt};

use common::async_rpc::{AsyncOpResult, AsyncOpStream, OpError};

use signature::canonical::CanonicalSerialize;

use crypto::hash::{hash_buffer, Hasher};
use crypto::identity::compare_public_key;

use identity::IdentityClient;

use proto::app_server::messages::RelayAddress;
use proto::crypto::{HashResult, PublicKey, RandValue, Signature, Uid};
use proto::funder::messages::{
    Currency, CurrencyOperations, McBalance, MoveToken, PendingTransaction, TokenInfo,
};

use signature::signature_buff::{hash_token_info, move_token_signature_buff};
use signature::verify::verify_move_token;

use database::interface::funder::CurrencyConfig;

use crate::mutual_credit::incoming::{
    process_operations_list, IncomingMessage, ProcessTransListError,
};
use crate::mutual_credit::outgoing::{queue_operation, QueueOperationError};
use crate::mutual_credit::types::McTransaction;

use crate::types::{create_hashed, MoveTokenHashed};

#[derive(Debug)]
pub enum TcOpError {
    SendOpFailed,
    ResponseOpFailed(oneshot::Canceled),
}

pub type TcOpResult<T> = Result<T, TcOpError>;
pub type TcOpSenderResult<T> = oneshot::Sender<TcOpResult<T>>;

// TODO: MoveTokenHashed contains too much information.
// We may remove some of the information, for example: local and remote public keys.
pub enum TcStatus<B> {
    ConsistentIn(MoveTokenHashed),
    ConsistentOut(MoveToken<B>, MoveTokenHashed),
    Inconsistent(Signature, u128), // (local_reset_token, local_reset_move_token_counter)
}

pub trait TcTransaction<B> {
    type McTransaction: McTransaction;
    fn mc_transaction(&mut self, currency: Currency) -> &mut Self::McTransaction;

    fn get_tc_status(&mut self) -> AsyncOpResult<TcStatus<B>>;
    fn set_direction_incoming(&mut self, move_token_hashed: MoveTokenHashed) -> AsyncOpResult<()>;
    fn set_direction_outgoing(&mut self, move_token: MoveToken<B>) -> AsyncOpResult<()>;
    fn set_direction_outgoing_empty_incoming(
        &mut self,
        move_token: MoveToken<B>,
    ) -> AsyncOpResult<()>;

    fn get_move_token_counter(&mut self) -> AsyncOpResult<u128>;
    fn set_move_token_counter(&mut self, move_token_counter: u128) -> AsyncOpResult<()>;

    fn get_currency_config(&mut self, currency: Currency) -> AsyncOpResult<CurrencyConfig>;

    /// Return a sorted async iterator of all balances
    fn list_balances(&mut self) -> AsyncOpStream<(Currency, McBalance)>;

    /// Return a sorted async iterator of all local reset proposal balances
    /// Only relevant for inconsistent channels
    fn list_local_reset_balances(&mut self) -> AsyncOpStream<(Currency, McBalance)>;

    fn is_local_currency(&mut self, currency: Currency) -> AsyncOpResult<bool>;
    fn is_remote_currency(&mut self, currency: Currency) -> AsyncOpResult<bool>;

    fn add_local_currency(&mut self, currency: Currency) -> AsyncOpResult<()>;
    fn remove_local_currency(&mut self, currency: Currency) -> AsyncOpResult<()>;

    fn add_remote_currency(&mut self, currency: Currency) -> AsyncOpResult<()>;
    fn remove_remote_currency(&mut self, currency: Currency) -> AsyncOpResult<()>;

    fn add_mutual_credit(&mut self, currency: Currency) -> AsyncOpResult<()>;
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
    InvalidTokenChannelStatus,
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
    pub move_token: MoveToken<B>,
}

#[derive(Debug, From)]
pub enum SendMoveTokenError {
    InvalidTokenChannelStatus,
    CanNotRemoveCurrencyInUse,
    MoveTokenCounterOverflow,
    QueueOperationError(QueueOperationError),
    OpError(OpError),
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
    let public_key_hash = Hasher::new().chain(public_key).finalize();
    RandValue::try_from(&public_key_hash.as_ref()[..RandValue::len()]).unwrap()
}

/// Create an initial move token in the relationship between two public keys.
/// To canonicalize the initial move token (Having an equal move token for both sides), we sort the
/// two public keys in some way.
fn initial_move_token<B>(low_public_key: &PublicKey, high_public_key: &PublicKey) -> MoveToken<B> {
    let token_info = TokenInfo {
        // No balances yet:
        balances_hash: hash_buffer(&[]),
        move_token_counter: 0,
    };

    // let info_hash = hash_token_info(remote_public_key, local_public_key, &token_info);

    // This is a special initialization case.
    // Note that this is the only case where new_token is not a valid signature.
    // We do this because we want to have synchronization between the two sides of the token
    // channel, however, the remote side has no means of generating the signature (Because he
    // doesn't have the private key). Therefore we use a dummy new_token instead.
    let move_token_out = MoveToken {
        old_token: token_from_public_key(&low_public_key),
        currencies_operations: Vec::new(),
        relays_diff: Vec::new(),
        currencies_diff: Vec::new(),
        info_hash: hash_token_info(&low_public_key, &high_public_key, &token_info),
        new_token: token_from_public_key(&high_public_key),
    };

    move_token_out
}

#[derive(Debug, From)]
pub enum InitTokenChannelError {
    InvalidTokenChannelStatus,
    InvalidResetMoveToken,
    InvalidTokenInfo,
    InvalidSignature,
    OpError(OpError),
}

/// Create a new token channel, and determines the initial state for the local side.
/// The initial state is determined according to the order between the two provided public keys.
/// Note that this function does not affect the database, it only returns a new state for the token
/// channel.
pub async fn init_token_channel<B>(
    tc_transaction: &mut impl TcTransaction<B>,
    local_public_key: &PublicKey,
    remote_public_key: &PublicKey,
) -> Result<(), InitTokenChannelError>
where
    B: CanonicalSerialize + Clone,
{
    Ok(
        if compare_public_key(&local_public_key, &remote_public_key) == Ordering::Less {
            // We are the first sender
            tc_transaction
                .set_direction_outgoing_empty_incoming(initial_move_token(
                    local_public_key,
                    remote_public_key,
                ))
                .await?
        } else {
            // We are the second sender
            let move_token_in = initial_move_token(remote_public_key, local_public_key);
            let token_info = TokenInfo {
                // No balances yet:
                balances_hash: hash_buffer(&[]),
                move_token_counter: 0,
            };
            tc_transaction
                .set_direction_incoming(create_hashed::<B>(&move_token_in, &token_info))
                .await?
        },
    )
}

/// Remote side has accepted our reset proposal
/// Adjust token channel accordingly.
pub async fn init_token_channel_from_remote_reset<B>(
    tc_transaction: &mut impl TcTransaction<B>,
    reset_move_token: &MoveToken<B>,
    local_public_key: &PublicKey,
    remote_public_key: &PublicKey,
) -> Result<(), InitTokenChannelError>
where
    B: Clone + CanonicalSerialize,
{
    // TODO: Should we verify the signature here?
    // Verify signature:
    if !verify_move_token(&reset_move_token, &remote_public_key) {
        return Err(InitTokenChannelError::InvalidSignature);
    }

    // Make sure that the MoveToken message is empty:
    if !reset_move_token.relays_diff.is_empty()
        || !reset_move_token.currencies_diff.is_empty()
        || !reset_move_token.currencies_operations.is_empty()
    {
        return Err(InitTokenChannelError::InvalidResetMoveToken);
    }

    let (local_reset_token, local_reset_move_token_counter) = match tc_transaction
        .get_tc_status()
        .await?
    {
        TcStatus::ConsistentIn(_) => return Err(InitTokenChannelError::InvalidTokenChannelStatus),

        TcStatus::ConsistentOut(_, _) => {
            return Err(InitTokenChannelError::InvalidTokenChannelStatus)
        }
        TcStatus::Inconsistent(local_reset_token, local_reset_move_token_counter) => {
            (local_reset_token, local_reset_move_token_counter)
        }
    };

    // Calculate `info_hash` as seen by the remote side.
    // Create what we expect to be TokenInfo (From the point of view of remote side):
    let mc_balances = tc_transaction.list_local_reset_balances();

    let token_info = TokenInfo {
        balances_hash: hash_mc_infos(
            mc_balances.map_ok(|(currency, mc_balance)| (currency, mc_balance.flip())),
        )
        .await?,
        move_token_counter: local_reset_move_token_counter,
    };

    let info_hash = hash_token_info(remote_public_key, local_public_key, &token_info);
    if reset_move_token.info_hash != info_hash {
        return Err(InitTokenChannelError::InvalidTokenInfo);
    }

    // Update move_token_counter:
    tc_transaction
        .set_move_token_counter(local_reset_move_token_counter)
        .await?;

    // Set direction to outgoing, with the newly received move token:
    let new_move_token_hashed = create_hashed(&reset_move_token, &token_info);
    tc_transaction
        .set_direction_incoming(new_move_token_hashed)
        .await?;

    Ok(())
}

async fn handle_in_move_token<B>(
    tc_transaction: &mut impl TcTransaction<B>,
    new_move_token: MoveToken<B>,
    local_public_key: &PublicKey,
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
                local_public_key,
                remote_public_key,
            )
            .await
        }
        TcStatus::Inconsistent(_local_reset_token, _local_reset_move_token_counter) => {
            return Err(ReceiveMoveTokenError::InvalidTokenChannelStatus);
        }
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
    local_public_key: &PublicKey,
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
                local_public_key,
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

// TODO: Should we move this function to offset-signature crate?
async fn hash_mc_infos(
    mut mc_infos: impl Stream<Item = Result<(Currency, McBalance), OpError>> + Unpin,
) -> Result<HashResult, OpError> {
    let mut hasher = Hasher::new();
    // Last seen currency, used to verify sorted order:
    let mut opt_last_currency: Option<Currency> = None;

    while let Some(item) = mc_infos.next().await {
        let (currency, mc_balance) = item?;
        // Ensure that the list is sorted:
        if let Some(last_currency) = opt_last_currency {
            assert!(last_currency <= currency);
        }
        // Update the last currency:
        opt_last_currency = Some(currency.clone());
        hasher.update(&hash_buffer(&currency.canonical_serialize()));
        hasher.update(&mc_balance.canonical_serialize());
    }
    Ok(hasher.finalize())
}

async fn handle_incoming_token_match<B>(
    tc_transaction: &mut impl TcTransaction<B>,
    move_token_out: MoveToken<B>,
    new_move_token: MoveToken<B>, // remote_max_debts: &ImHashMap<Currency, u128>,
    local_public_key: &PublicKey,
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
    if !verify_move_token(&new_move_token, &remote_public_key) {
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
    //  - (1) in the xor set.
    //  - (2) in the remote set
    //  - (3) (Not in the local set) or (in the local set with balance zero and zero pending debts)
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
                // TODO: Do We need to report outside that we removed this currency somehow?
                // TODO: Somehow unite all code for removing remote currencies.
                } else {
                    // We are not allowed to remove this currency!
                    return Err(ReceiveMoveTokenError::CanNotRemoveCurrencyInUse);
                }
            } else {
                tc_transaction
                    .remove_remote_currency(diff_currency.clone())
                    .await?;
                // TODO: Do We need to report outside that we removed this currency somehow?
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

    let move_token_counter = tc_transaction.get_move_token_counter().await?;
    let new_move_token_counter = move_token_counter
        .checked_add(1)
        .ok_or(ReceiveMoveTokenError::MoveTokenCounterOverflow)?;

    // Calculate `info_hash` as seen by the remote side.
    // Create what we expect to be TokenInfo (From the point of view of remote side):
    let mc_balances = tc_transaction.list_balances();

    let token_info = TokenInfo {
        balances_hash: hash_mc_infos(
            mc_balances.map_ok(|(currency, mc_balance)| (currency, mc_balance.flip())),
        )
        .await?,
        move_token_counter: new_move_token_counter,
    };

    let info_hash = hash_token_info(remote_public_key, local_public_key, &token_info);

    if new_move_token.info_hash != info_hash {
        return Err(ReceiveMoveTokenError::InvalidTokenInfo);
    }

    // Update move_token_counter:
    tc_transaction
        .set_move_token_counter(new_move_token_counter)
        .await?;

    // Set direction to outgoing, with the newly received move token:
    let new_move_token_hashed = create_hashed(&new_move_token, &token_info);
    tc_transaction
        .set_direction_incoming(new_move_token_hashed)
        .await?;

    Ok(move_token_received)
}

async fn handle_out_move_token<B>(
    tc_transaction: &mut impl TcTransaction<B>,
    identity_client: &mut IdentityClient,
    currencies_operations: Vec<CurrencyOperations>,
    relays_diff: Vec<RelayAddress<B>>,
    currencies_diff: Vec<Currency>,
    local_public_key: &PublicKey,
    remote_public_key: &PublicKey,
) -> Result<MoveToken<B>, SendMoveTokenError>
where
    B: CanonicalSerialize + Clone,
{
    // We expect that our current state is incoming:
    let move_token_in = match tc_transaction.get_tc_status().await? {
        TcStatus::ConsistentIn(move_token_in) => move_token_in,
        TcStatus::ConsistentOut(_move_token_out, _move_token_in) => {
            return Err(SendMoveTokenError::InvalidTokenChannelStatus)
        }
        TcStatus::Inconsistent(_local_reset_token, _local_reset_move_token_counter) => {
            return Err(SendMoveTokenError::InvalidTokenChannelStatus)
        }
    };

    // Handle currencies_diff:
    for diff_currency in &currencies_diff {
        // Check if we need to add currency:
        if !tc_transaction
            .is_local_currency(diff_currency.clone())
            .await?
        {
            // Add a new local currency.
            tc_transaction
                .add_local_currency(diff_currency.clone())
                .await?;

            // If we also have this currency as a remote currency, add a new mutual credit:
            if tc_transaction
                .is_remote_currency(diff_currency.clone())
                .await?
            {
                tc_transaction.add_mutual_credit(diff_currency.clone());
            }
        } else {
            let is_remote_currency = tc_transaction
                .is_remote_currency(diff_currency.clone())
                .await?;
            if is_remote_currency {
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
                        .remove_local_currency(diff_currency.clone())
                        .await?;
                // TODO: Do We need to report outside that we removed this currency somehow?
                // TODO: Somehow unite all code for removing local currencies.
                } else {
                    // We are not allowed to remove this currency!
                    // TODO: Should be an error instead?
                    return Err(SendMoveTokenError::CanNotRemoveCurrencyInUse);
                }
            } else {
                tc_transaction
                    .remove_remote_currency(diff_currency.clone())
                    .await?;
                // TODO: Do We need to report outside that we removed this currency somehow?
            }
        }
    }

    // Update mutual credits:
    for currency_operations in &currencies_operations {
        for operation in currency_operations.operations.iter().cloned() {
            queue_operation(
                tc_transaction.mc_transaction(currency_operations.currency.clone()),
                operation,
                &currency_operations.currency,
                local_public_key,
            )
            .await?;
        }
    }

    let new_move_token_counter = tc_transaction
        .get_move_token_counter()
        .await?
        .checked_add(1)
        .ok_or(SendMoveTokenError::MoveTokenCounterOverflow)?;
    let mc_balances = tc_transaction.list_balances();

    let token_info = TokenInfo {
        balances_hash: hash_mc_infos(mc_balances).await?,
        move_token_counter: new_move_token_counter,
    };

    let mut move_token = MoveToken {
        old_token: move_token_in.new_token,
        currencies_operations,
        relays_diff,
        currencies_diff,
        info_hash: hash_token_info(local_public_key, remote_public_key, &token_info),
        // Still not known:
        new_token: Signature::from(&[0; Signature::len()]),
    };

    // Fill in signature:
    let signature_buff = move_token_signature_buff(&move_token);
    move_token.new_token = identity_client
        .request_signature(signature_buff)
        .await
        .unwrap();

    // Set the direction to be outgoing:
    // TODO: This should also save the last incoming move token, in an atomic way.
    // Should probably be implemented inside the database code.
    tc_transaction
        .set_direction_outgoing(move_token.clone())
        .await?;

    Ok(move_token)
}

// TODO: Implement: new_from_local_reset, new_from_remote_reset.

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
