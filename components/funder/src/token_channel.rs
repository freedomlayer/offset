use std::cmp::Ordering;
use std::convert::TryFrom;
use std::iter::IntoIterator;

use derive_more::From;

use futures::channel::oneshot;
use futures::{Stream, StreamExt, TryStreamExt};

use common::async_rpc::{AsyncOpResult, AsyncOpStream, OpError};

use crypto::hash::{hash_buffer, Hasher};
use crypto::identity::compare_public_key;

use identity::IdentityClient;

use proto::app_server::messages::RelayAddress;
use proto::crypto::{HashResult, PublicKey, RandValue, Signature, Uid};
use proto::funder::messages::{
    Currency, CurrencyOperations, McBalance, MoveToken, PendingTransaction, TokenInfo,
};

use signature::canonical::CanonicalSerialize;
use signature::signature_buff::{
    hash_token_info, move_token_signature_buff, reset_token_signature_buff,
};
use signature::verify::verify_move_token;

use database::interface::funder::CurrencyConfig;
use database::transaction::Transaction;

use crate::mutual_credit::incoming::{
    process_operations_list, IncomingMessage, ProcessTransListError,
};
use crate::mutual_credit::outgoing::{queue_operation, QueueOperationError};
use crate::mutual_credit::types::McClient;

use crate::types::{create_hashed, MoveTokenHashed};

#[derive(Debug)]
pub enum TcOpError {
    SendOpFailed,
    ResponseOpFailed(oneshot::Canceled),
}

pub type TcOpResult<T> = Result<T, TcOpError>;
pub type TcOpSenderResult<T> = oneshot::Sender<TcOpResult<T>>;

/// Status of a TokenChannel. Could be either outgoing, incoming or inconsistent.
pub enum TcStatus<B> {
    ConsistentIn(MoveTokenHashed),                // (move_token_in)
    ConsistentOut(MoveToken<B>, MoveTokenHashed), // (move_token_out, last_move_token_in)
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

/// Unrecoverable TokenChannel error
#[derive(Debug, From)]
pub enum TokenChannelError {
    InvalidTransaction(ProcessTransListError),
    MoveTokenCounterOverflow,
    CanNotRemoveCurrencyInUse,
    InvalidTokenChannelStatus,
    RequestSignatureError,
    OpError(OpError),
    QueueOperationError(QueueOperationError),
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
    ChainInconsistent(Signature, u128), // (local_reset_token, local_reset_move_token_counter)
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

    // This is a special initialization case.
    // Note that this is the only case where new_token is not a valid signature.
    // We do this because we want to have synchronization between the two sides of the token
    // channel, however, the remote side has no means of generating the signature (Because he
    // doesn't have the private key). Therefore we use a dummy new_token instead.
    let move_token_out = MoveToken::<B> {
        old_token: token_from_public_key(&low_public_key),
        currencies_operations: Vec::new(),
        relays_diff: Vec::new(),
        currencies_diff: Vec::new(),
        info_hash: hash_token_info(&low_public_key, &high_public_key, &token_info),
        new_token: token_from_public_key(&high_public_key),
    };

    move_token_out
}

/// Create a new token channel, and determines the initial state for the local side.
/// The initial state is determined according to the order between the two provided public keys.
/// Note that this function does not affect the database, it only returns a new state for the token
/// channel.
pub async fn init_token_channel<B>(
    tc_client: &mut impl TcClient<B>,
    local_public_key: &PublicKey,
    remote_public_key: &PublicKey,
) -> Result<(), TokenChannelError>
where
    B: CanonicalSerialize + Clone,
{
    Ok(
        if compare_public_key(&local_public_key, &remote_public_key) == Ordering::Less {
            // We are the first sender
            tc_client
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
            tc_client
                .set_direction_incoming(create_hashed::<B>(&move_token_in, &token_info))
                .await?
        },
    )
}

pub async fn handle_in_move_token<B, C>(
    tc_client: &mut C,
    identity_client: &mut IdentityClient,
    new_move_token: MoveToken<B>,
    local_public_key: &PublicKey,
    remote_public_key: &PublicKey,
) -> Result<ReceiveMoveTokenOutput<B>, TokenChannelError>
where
    // TODO: Can we somehow get rid of the Sync requirement for `B`?
    B: CanonicalSerialize + Clone + Send + Sync,
    C: TcClient<B> + Transaction + Send,
    C::McClient: Send,
{
    match tc_client.get_tc_status().await? {
        TcStatus::ConsistentIn(move_token_in) => {
            handle_in_move_token_dir_in(
                tc_client,
                identity_client,
                local_public_key,
                remote_public_key,
                move_token_in,
                new_move_token,
            )
            .await
        }
        TcStatus::ConsistentOut(move_token_out, _move_token_in) => {
            handle_in_move_token_dir_out(
                tc_client,
                identity_client,
                move_token_out,
                new_move_token,
                local_public_key,
                remote_public_key,
            )
            .await
        }
        TcStatus::Inconsistent(local_reset_token, local_reset_move_token_counter, _opt_remote) => {
            // Might be a reset move token
            if new_move_token.old_token == local_reset_token {
                // This is a reset move token!

                // Simulate an outgoing move token with the correct `new_token`:
                let token_info = TokenInfo {
                    balances_hash: hash_mc_infos(tc_client.list_local_reset_balances()).await?,
                    move_token_counter: local_reset_move_token_counter
                        .checked_sub(1)
                        .ok_or(TokenChannelError::MoveTokenCounterOverflow)?,
                };

                let move_token_out = MoveToken {
                    old_token: Signature::from(&[0; Signature::len()]),
                    currencies_operations: Vec::new(),
                    relays_diff: Vec::new(),
                    currencies_diff: Vec::new(),
                    info_hash: hash_token_info(local_public_key, remote_public_key, &token_info),
                    new_token: local_reset_token.clone(),
                };

                // Atomically attempt to handle a reset move token:
                let (output, _was_committed) = tc_client
                    .transaction(|tc_client| {
                        Box::pin(async move {
                            let output = async move {
                                tc_client
                                    .set_outgoing_from_inconsistent(move_token_out.clone())
                                    .await?;

                                // Attempt to receive an incoming token:
                                handle_incoming_token_match(
                                    tc_client,
                                    move_token_out,
                                    new_move_token,
                                    local_public_key,
                                    remote_public_key,
                                )
                                .await
                            }
                            .await;

                            // Decide whether we should commit the transaction
                            let should_commit = match &output {
                                Ok(IncomingTokenMatchOutput::MoveTokenReceived(_)) => true,
                                Ok(IncomingTokenMatchOutput::InvalidIncoming(_)) => false,
                                Err(_) => false,
                            };
                            (output, should_commit)
                        })
                    })
                    .await;

                match output? {
                    IncomingTokenMatchOutput::MoveTokenReceived(move_token_received) => {
                        Ok(ReceiveMoveTokenOutput::Received(move_token_received))
                    }
                    IncomingTokenMatchOutput::InvalidIncoming(_) => {
                        // In this case the transaction was not committed:
                        Ok(ReceiveMoveTokenOutput::ChainInconsistent(
                            local_reset_token,
                            local_reset_move_token_counter,
                        ))
                    }
                }
            } else {
                Ok(ReceiveMoveTokenOutput::ChainInconsistent(
                    local_reset_token,
                    local_reset_move_token_counter,
                ))
            }
        }
    }
}

/// Generate a reset token, to be used by remote side if he wants to accept the reset terms.
async fn create_reset_token(
    identity_client: &mut IdentityClient,
    local_public_key: &PublicKey,
    remote_public_key: &PublicKey,
    move_token_counter: u128,
) -> Result<Signature, TokenChannelError> {
    // Some ideas:
    // - Use a random generator to randomly generate an identity client.
    // - Sign over a blob that contains:
    //      - hash(prefix ("SOME_STRING"))
    //      - Both public keys.
    //      - local_reset_move_token_counter
    let sign_buffer =
        reset_token_signature_buff(local_public_key, remote_public_key, move_token_counter);
    Ok(identity_client
        .request_signature(sign_buffer)
        .await
        .map_err(|_| TokenChannelError::RequestSignatureError)?)
}

async fn set_inconsistent<B>(
    tc_client: &mut impl TcClient<B>,
    identity_client: &mut IdentityClient,
    local_public_key: &PublicKey,
    remote_public_key: &PublicKey,
) -> Result<(Signature, u128), TokenChannelError> {
    // We increase by 2, because the other side might have already signed a move token that equals
    // to our move token plus 1, so he might not be willing to sign another move token with the
    // same `move_token_counter`. There could be a gap of size `1` as a result, but we don't care
    // about it, as long as the `move_token_counter` values are monotonic.
    let local_reset_move_token_counter = tc_client
        .get_move_token_counter()
        .await?
        .checked_add(2)
        .ok_or(TokenChannelError::MoveTokenCounterOverflow)?;

    // Create a local reset token, to be used by the remote side to accept our terms:
    let local_reset_token = create_reset_token(
        identity_client,
        local_public_key,
        remote_public_key,
        local_reset_move_token_counter,
    )
    .await?;

    tc_client
        .set_inconsistent(
            local_reset_token.clone(),
            local_reset_move_token_counter.clone(),
        )
        .await?;

    Ok((local_reset_token, local_reset_move_token_counter))
}

async fn handle_in_move_token_dir_in<B>(
    tc_client: &mut impl TcClient<B>,
    identity_client: &mut IdentityClient,
    local_public_key: &PublicKey,
    remote_public_key: &PublicKey,
    move_token_in: MoveTokenHashed,
    new_move_token: MoveToken<B>,
) -> Result<ReceiveMoveTokenOutput<B>, TokenChannelError>
where
    B: CanonicalSerialize + Clone,
{
    if move_token_in == create_hashed(&new_move_token, &move_token_in.token_info) {
        // Duplicate
        Ok(ReceiveMoveTokenOutput::Duplicate)
    } else {
        // Inconsistency
        let (local_reset_token, local_reset_move_token_counter) = set_inconsistent(
            tc_client,
            identity_client,
            local_public_key,
            remote_public_key,
        )
        .await?;
        Ok(ReceiveMoveTokenOutput::ChainInconsistent(
            local_reset_token,
            local_reset_move_token_counter,
        ))
    }
}

async fn handle_in_move_token_dir_out<B, C>(
    tc_client: &mut C,
    identity_client: &mut IdentityClient,
    move_token_out: MoveToken<B>,
    new_move_token: MoveToken<B>,
    local_public_key: &PublicKey,
    remote_public_key: &PublicKey,
) -> Result<ReceiveMoveTokenOutput<B>, TokenChannelError>
where
    B: Clone + CanonicalSerialize + Send + Sync,
    C: TcClient<B> + Transaction + Send,
    C::McClient: Send,
{
    if new_move_token.old_token == move_token_out.new_token {
        // Atomically attempt to handle an incoming move token:
        let (output, _was_committed) = tc_client
            .transaction(|tc_client| {
                Box::pin(async move {
                    let output = async move {
                        // Attempt to receive an incoming token:
                        handle_incoming_token_match(
                            tc_client,
                            move_token_out,
                            new_move_token,
                            local_public_key,
                            remote_public_key,
                        )
                        .await
                    }
                    .await;

                    // Decide whether we should commit the transaction
                    let should_commit = match &output {
                        Ok(IncomingTokenMatchOutput::MoveTokenReceived(_)) => true,
                        Ok(IncomingTokenMatchOutput::InvalidIncoming(_)) => false,
                        Err(_) => false,
                    };
                    (output, should_commit)
                })
            })
            .await;

        match output? {
            IncomingTokenMatchOutput::MoveTokenReceived(move_token_received) => {
                return Ok(ReceiveMoveTokenOutput::Received(move_token_received))
            }
            IncomingTokenMatchOutput::InvalidIncoming(_) => {
                // Inconsistency
                // In this case the transaction was not committed:
                let (local_reset_token, local_reset_move_token_counter) = set_inconsistent(
                    tc_client,
                    identity_client,
                    local_public_key,
                    remote_public_key,
                )
                .await?;
                Ok(ReceiveMoveTokenOutput::ChainInconsistent(
                    local_reset_token,
                    local_reset_move_token_counter,
                ))
            }
        }
    // self.outgoing_to_incoming(friend_move_token, new_move_token)
    } else if move_token_out.old_token == new_move_token.new_token {
        // We should retransmit our move token message to the remote side.
        Ok(ReceiveMoveTokenOutput::RetransmitOutgoing(
            move_token_out.clone(),
        ))
    } else {
        // Inconsistency
        let (local_reset_token, local_reset_move_token_counter) = set_inconsistent(
            tc_client,
            identity_client,
            local_public_key,
            remote_public_key,
        )
        .await?;
        Ok(ReceiveMoveTokenOutput::ChainInconsistent(
            local_reset_token,
            local_reset_move_token_counter,
        ))
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

#[derive(Debug)]
enum InvalidIncoming {
    InvalidSignature,
    InvalidOperation,
    InvalidTokenInfo,
    CanNotRemoveCurrencyInUse,
}

#[derive(Debug)]
enum IncomingTokenMatchOutput<B> {
    MoveTokenReceived(MoveTokenReceived<B>),
    InvalidIncoming(InvalidIncoming),
}

async fn handle_incoming_token_match<B>(
    tc_client: &mut impl TcClient<B>,
    move_token_out: MoveToken<B>,
    new_move_token: MoveToken<B>, // remote_max_debts: &ImHashMap<Currency, u128>,
    local_public_key: &PublicKey,
    remote_public_key: &PublicKey,
) -> Result<IncomingTokenMatchOutput<B>, TokenChannelError>
where
    B: Clone + CanonicalSerialize,
{
    // Verify signature:
    // Note that we only verify the signature here, and not at the Incoming part.
    // This allows the genesis move token to occur smoothly, even though its signature
    // is not correct.
    // TODO: Check if the above statement is still true.
    if !verify_move_token(&new_move_token, &remote_public_key) {
        return Ok(IncomingTokenMatchOutput::InvalidIncoming(
            InvalidIncoming::InvalidSignature,
        ));
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
    // - Add to remote currencies every currency that is in the xor set, but not in the current
    // set.
    // - Remove from remote currencies currencies that satisfy the following requirements:
    //  - (1) in the xor set.
    //  - (2) in the remote set
    //  - (3) (Not in the local set) or (in the local set with balance zero and zero pending debts)
    for diff_currency in &new_move_token.currencies_diff {
        // Check if we need to add currency:
        if !tc_client.is_remote_currency(diff_currency.clone()).await? {
            // Add a new remote currency.
            tc_client.add_remote_currency(diff_currency.clone()).await?;

            // If we also have this currency as a local currency, add a new mutual credit:
            if tc_client.is_local_currency(diff_currency.clone()).await? {
                tc_client.add_mutual_credit(diff_currency.clone());
            }
        } else {
            let is_local_currency = tc_client.is_local_currency(diff_currency.clone()).await?;
            if is_local_currency {
                let mc_balance = tc_client
                    .mc_client(diff_currency.clone())
                    .get_balance()
                    .await?;
                if mc_balance.balance == 0
                    && mc_balance.remote_pending_debt == 0
                    && mc_balance.local_pending_debt == 0
                {
                    // We may remove the currency if the balance and pending debts are exactly
                    // zero:
                    tc_client
                        .remove_remote_currency(diff_currency.clone())
                        .await?;
                // TODO: Do We need to report outside that we removed this currency somehow?
                // TODO: Somehow unite all code for removing remote currencies.
                } else {
                    // We are not allowed to remove this currency!
                    return Ok(IncomingTokenMatchOutput::InvalidIncoming(
                        InvalidIncoming::CanNotRemoveCurrencyInUse,
                    ));
                }
            } else {
                tc_client
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
        let remote_max_debt = tc_client
            .get_currency_config(currency_operations.currency.clone())
            .await?
            .remote_max_debt;

        let res = process_operations_list(
            tc_client.mc_client(currency_operations.currency.clone()),
            currency_operations.operations.clone(),
            &currency_operations.currency,
            remote_public_key,
            remote_max_debt,
        )
        .await;

        let incoming_messages = match res {
            Ok(incoming_messages) => incoming_messages,
            Err(_) => {
                return Ok(IncomingTokenMatchOutput::InvalidIncoming(
                    InvalidIncoming::InvalidOperation,
                ))
            }
        };

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

    let move_token_counter = tc_client.get_move_token_counter().await?;
    let new_move_token_counter = move_token_counter
        .checked_add(1)
        .ok_or(TokenChannelError::MoveTokenCounterOverflow)?;

    // Calculate `info_hash` as seen by the remote side.
    // Create what we expect to be TokenInfo (From the point of view of remote side):
    let mc_balances = tc_client.list_balances();

    let token_info = TokenInfo {
        balances_hash: hash_mc_infos(
            mc_balances.map_ok(|(currency, mc_balance)| (currency, mc_balance.flip())),
        )
        .await?,
        move_token_counter: new_move_token_counter,
    };

    let info_hash = hash_token_info(remote_public_key, local_public_key, &token_info);

    if new_move_token.info_hash != info_hash {
        return Ok(IncomingTokenMatchOutput::InvalidIncoming(
            InvalidIncoming::InvalidTokenInfo,
        ));
    }

    // Update move_token_counter:
    tc_client
        .set_move_token_counter(new_move_token_counter)
        .await?;

    // Set direction to outgoing, with the newly received move token:
    let new_move_token_hashed = create_hashed(&new_move_token, &token_info);
    tc_client
        .set_direction_incoming(new_move_token_hashed)
        .await?;

    Ok(IncomingTokenMatchOutput::MoveTokenReceived(
        move_token_received,
    ))
}

pub async fn handle_out_move_token<B>(
    tc_client: &mut impl TcClient<B>,
    identity_client: &mut IdentityClient,
    currencies_operations: Vec<CurrencyOperations>,
    relays_diff: Vec<RelayAddress<B>>,
    currencies_diff: Vec<Currency>,
    local_public_key: &PublicKey,
    remote_public_key: &PublicKey,
) -> Result<MoveToken<B>, TokenChannelError>
where
    B: CanonicalSerialize + Clone,
{
    // We expect that our current state is incoming:
    let move_token_in = match tc_client.get_tc_status().await? {
        TcStatus::ConsistentIn(move_token_in) => move_token_in,
        TcStatus::ConsistentOut(_move_token_out, _move_token_in) => {
            return Err(TokenChannelError::InvalidTokenChannelStatus)
        }
        TcStatus::Inconsistent(
            _local_reset_token,
            _local_reset_move_token_counter,
            _opt_remote,
        ) => {
            return Err(TokenChannelError::InvalidTokenChannelStatus);
        }
    };

    // Handle currencies_diff:
    for diff_currency in &currencies_diff {
        // Check if we need to add currency:
        if !tc_client.is_local_currency(diff_currency.clone()).await? {
            // Add a new local currency.
            tc_client.add_local_currency(diff_currency.clone()).await?;

            // If we also have this currency as a remote currency, add a new mutual credit:
            if tc_client.is_remote_currency(diff_currency.clone()).await? {
                tc_client.add_mutual_credit(diff_currency.clone());
            }
        } else {
            let is_remote_currency = tc_client.is_remote_currency(diff_currency.clone()).await?;
            if is_remote_currency {
                let mc_balance = tc_client
                    .mc_client(diff_currency.clone())
                    .get_balance()
                    .await?;
                if mc_balance.balance == 0
                    && mc_balance.remote_pending_debt == 0
                    && mc_balance.local_pending_debt == 0
                {
                    // We may remove the currency if the balance and pending debts are exactly
                    // zero:
                    tc_client
                        .remove_local_currency(diff_currency.clone())
                        .await?;
                // TODO: Do We need to report outside that we removed this currency somehow?
                // TODO: Somehow unite all code for removing local currencies.
                } else {
                    // We are not allowed to remove this currency!
                    // TODO: Should be an error instead?
                    return Err(TokenChannelError::CanNotRemoveCurrencyInUse);
                }
            } else {
                tc_client
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
                tc_client.mc_client(currency_operations.currency.clone()),
                operation,
                &currency_operations.currency,
                local_public_key,
            )
            .await?;
        }
    }

    let new_move_token_counter = tc_client
        .get_move_token_counter()
        .await?
        .checked_add(1)
        .ok_or(TokenChannelError::MoveTokenCounterOverflow)?;
    let mc_balances = tc_client.list_balances();

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
        .map_err(|_| TokenChannelError::RequestSignatureError)?;

    // Set the direction to be outgoing:
    // TODO: This should also save the last incoming move token, in an atomic way.
    // Should probably be implemented inside the database code.
    tc_client.set_direction_outgoing(move_token.clone()).await?;

    Ok(move_token)
}

/// Apply a token channel reset, accepting remote side's
/// reset terms.
pub async fn accept_remote_reset<B>(
    tc_client: &mut impl TcClient<B>,
    identity_client: &mut IdentityClient,
    currencies_operations: Vec<CurrencyOperations>,
    relays_diff: Vec<RelayAddress<B>>,
    currencies_diff: Vec<Currency>,
    local_public_key: &PublicKey,
    remote_public_key: &PublicKey,
) -> Result<MoveToken<B>, TokenChannelError>
where
    B: CanonicalSerialize + Clone,
{
    // Make sure that we are in inconsistent state,
    // and that the remote side has already sent his reset terms:
    let (remote_reset_token, remote_reset_move_token_counter) = match tc_client
        .get_tc_status()
        .await?
    {
        TcStatus::ConsistentIn(_) => return Err(TokenChannelError::InvalidTokenChannelStatus),
        TcStatus::ConsistentOut(_, _) => return Err(TokenChannelError::InvalidTokenChannelStatus),
        TcStatus::Inconsistent(_, _, None) => {
            // We don't have the remote side's reset terms yet:
            return Err(TokenChannelError::InvalidTokenChannelStatus);
        }
        TcStatus::Inconsistent(
            _local_reset_token,
            _local_reset_move_token_counter,
            Some((remote_reset_token, remote_reset_move_token_counter)),
        ) => (remote_reset_token, remote_reset_move_token_counter),
    };

    // Simulate an incoming move token with the correct `new_token`:
    let token_info = TokenInfo {
        balances_hash: hash_mc_infos(tc_client.list_remote_reset_balances()).await?,
        move_token_counter: remote_reset_move_token_counter
            .checked_sub(1)
            .ok_or(TokenChannelError::MoveTokenCounterOverflow)?,
    };

    let move_token_in = MoveToken::<B> {
        old_token: Signature::from(&[0; Signature::len()]),
        currencies_operations: Vec::new(),
        relays_diff: Vec::new(),
        currencies_diff: Vec::new(),
        info_hash: hash_token_info(local_public_key, remote_public_key, &token_info),
        new_token: remote_reset_token.clone(),
    };

    tc_client
        .set_incoming_from_inconsistent(create_hashed(&move_token_in, &token_info))
        .await?;

    // Create an outgoing move token, to be sent to the remote side:
    handle_out_move_token(
        tc_client,
        identity_client,
        currencies_operations,
        relays_diff,
        currencies_diff,
        local_public_key,
        remote_public_key,
    )
    .await
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
