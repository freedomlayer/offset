use std::cmp::Ordering;
use std::collections::HashMap;
use std::convert::TryFrom;

use derive_more::From;

use futures::{Stream, StreamExt, TryStreamExt};

use common::async_rpc::OpError;

use crypto::hash::{hash_buffer, Hasher};
use crypto::identity::compare_public_key;

use identity::IdentityClient;

use proto::app_server::messages::RelayAddress;
use proto::crypto::{HashResult, PublicKey, RandValue, Signature};
use proto::funder::messages::{Currency, CurrencyOperations, McBalance, MoveToken, TokenInfo};

use signature::canonical::CanonicalSerialize;
use signature::signature_buff::{
    hash_token_info, move_token_signature_buff, reset_token_signature_buff,
};
use signature::verify::verify_move_token;

use database::transaction::Transaction;

use crate::mutual_credit::incoming::{
    process_operations_list, IncomingMessage, ProcessTransListError,
};
use crate::mutual_credit::outgoing::{queue_operation, QueueOperationError};
use crate::mutual_credit::types::McClient;

use crate::token_channel::types::{ResetBalance, ResetTerms, TcClient, TcStatus};
use crate::types::{create_hashed, MoveTokenHashed};

/// Unrecoverable TokenChannel error
#[derive(Debug, From)]
pub enum TokenChannelError {
    InvalidTransaction(ProcessTransListError),
    MoveTokenCounterOverflow,
    CanNotRemoveCurrencyInUse,
    InvalidTokenChannelStatus,
    RequestSignatureError,
    InvalidDbState,
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
    pub currencies: Vec<MoveTokenReceivedCurrency>,
    pub relays_diff: Vec<RelayAddress<B>>,
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
pub enum ReceiveMoveTokenOutput<B> {
    Duplicate,
    RetransmitOutgoing(MoveToken<B>),
    Received(MoveTokenReceived<B>),
    ChainInconsistent(ResetTerms), // (local_reset_token, local_reset_move_token_counter)
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
    let move_token_counter = 0;
    Ok(
        if compare_public_key(&local_public_key, &remote_public_key) == Ordering::Less {
            // We are the first sender
            tc_client
                .set_direction_outgoing_empty_incoming(
                    initial_move_token(local_public_key, remote_public_key),
                    move_token_counter,
                )
                .await?
        } else {
            // We are the second sender
            let move_token_in = initial_move_token(remote_public_key, local_public_key);
            let token_info = TokenInfo {
                // No balances yet:
                balances_hash: hash_buffer(&[]),
                move_token_counter,
            };
            tc_client
                .set_direction_incoming(create_hashed::<B>(&move_token_in, &token_info))
                .await?
        },
    )
}

/// Create a new McBalance (with zero pending fees) based on a given ResetBalance
pub fn reset_balance_to_mc_balance(reset_balance: ResetBalance) -> McBalance {
    McBalance {
        balance: reset_balance.balance,
        local_pending_debt: 0,
        remote_pending_debt: 0,
        in_fees: reset_balance.in_fees,
        out_fees: reset_balance.out_fees,
    }
}

/// Extract all local balances for reset as a map:
async fn local_balances_for_reset<B>(
    tc_client: &mut impl TcClient<B>,
) -> Result<HashMap<Currency, ResetBalance>, TokenChannelError> {
    let mut balances = HashMap::new();
    let mut reset_balances = tc_client.list_local_reset_balances();
    while let Some(item) = reset_balances.next().await {
        let (currency, reset_balance) = item?;
        if let Some(_) = balances.insert(currency, reset_balance) {
            return Err(TokenChannelError::InvalidDbState);
        }
    }
    Ok(balances)
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
        TcStatus::ConsistentOut(move_token_out, _opt_move_token_in) => {
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
                    balances_hash: hash_mc_infos(tc_client.list_local_reset_balances().map_ok(
                        |(currency, mc_balance)| {
                            (currency, reset_balance_to_mc_balance(mc_balance))
                        },
                    ))
                    .await?,
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
                        Ok(ReceiveMoveTokenOutput::ChainInconsistent(ResetTerms {
                            reset_token: local_reset_token,
                            move_token_counter: local_reset_move_token_counter,
                            reset_balances: local_balances_for_reset(tc_client).await?,
                        }))
                    }
                }
            } else {
                Ok(ReceiveMoveTokenOutput::ChainInconsistent(ResetTerms {
                    reset_token: local_reset_token,
                    move_token_counter: local_reset_move_token_counter,
                    reset_balances: local_balances_for_reset(tc_client).await?,
                }))
            }
        }
    }
}

// TODO: Think abou the security implications of the implementation here.
// Some previous ideas:
// - Use a random generator to randomly generate an identity client.
// - Sign over a blob that contains:
//      - hash(prefix ("SOME_STRING"))
//      - Both public keys.
//      - local_reset_move_token_counter

/// Generate a reset token, to be used by remote side if he wants to accept the reset terms.
async fn create_reset_token(
    identity_client: &mut IdentityClient,
    local_public_key: &PublicKey,
    remote_public_key: &PublicKey,
    move_token_counter: u128,
) -> Result<Signature, TokenChannelError> {
    let sign_buffer =
        reset_token_signature_buff(local_public_key, remote_public_key, move_token_counter);
    Ok(identity_client
        .request_signature(sign_buffer)
        .await
        .map_err(|_| TokenChannelError::RequestSignatureError)?)
}

/// Set token channel to be inconsistent
/// Local reset terms are automatically calculated
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

        Ok(ReceiveMoveTokenOutput::ChainInconsistent(ResetTerms {
            reset_token: local_reset_token,
            move_token_counter: local_reset_move_token_counter,
            reset_balances: local_balances_for_reset(tc_client).await?,
        }))
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
                Ok(ReceiveMoveTokenOutput::ChainInconsistent(ResetTerms {
                    reset_token: local_reset_token,
                    move_token_counter: local_reset_move_token_counter,
                    reset_balances: local_balances_for_reset(tc_client).await?,
                }))
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
        Ok(ReceiveMoveTokenOutput::ChainInconsistent(ResetTerms {
            reset_token: local_reset_token,
            move_token_counter: local_reset_move_token_counter,
            reset_balances: local_balances_for_reset(tc_client).await?,
        }))
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
            .get_remote_max_debt(currency_operations.currency.clone())
            .await?;

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
        TcStatus::ConsistentOut(..) | TcStatus::Inconsistent(..) => {
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
    tc_client
        .set_direction_outgoing(move_token.clone(), new_move_token_counter)
        .await?;

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
    let (remote_reset_token, remote_reset_move_token_counter) =
        match tc_client.get_tc_status().await? {
            TcStatus::ConsistentIn(..)
            | TcStatus::ConsistentOut(..)
            | TcStatus::Inconsistent(_, _, None) => {
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
        balances_hash: hash_mc_infos(tc_client.list_remote_reset_balances().map_ok(
            |(currency, reset_balance)| (currency, reset_balance_to_mc_balance(reset_balance)),
        ))
        .await?,
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

/// Load the remote reset terms information from a remote side's inconsistency message
/// Optionally returns local reset terms (If not already inconsistent)
pub async fn load_remote_reset_terms<B>(
    tc_client: &mut impl TcClient<B>,
    identity_client: &mut IdentityClient,
    remote_reset_token: Signature,
    remote_reset_move_token_counter: u128,
    remote_reset_balances: impl IntoIterator<Item = (Currency, ResetBalance)>,
    local_public_key: &PublicKey,
    remote_public_key: &PublicKey,
) -> Result<Option<ResetTerms>, TokenChannelError> {
    // Check our current state:
    let ret_val = match tc_client.get_tc_status().await? {
        TcStatus::ConsistentIn(..) | TcStatus::ConsistentOut(..) => {
            // Change our token channel status to inconsistent:
            let (local_reset_token, local_reset_move_token_counter) = set_inconsistent(
                tc_client,
                identity_client,
                local_public_key,
                remote_public_key,
            )
            .await?;

            Some(ResetTerms {
                reset_token: local_reset_token,
                move_token_counter: local_reset_move_token_counter,
                reset_balances: local_balances_for_reset(tc_client).await?,
            })
        }
        TcStatus::Inconsistent(..) => None,
    };

    // Set remote reset terms:
    tc_client
        .set_inconsistent_remote_terms(remote_reset_token, remote_reset_move_token_counter)
        .await?;

    for (currency, mc_balance) in remote_reset_balances.into_iter() {
        tc_client
            .add_remote_reset_balance(currency, mc_balance)
            .await?;
    }

    Ok(ret_val)
}
