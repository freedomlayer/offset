use std::cmp::Ordering;
use std::collections::HashMap;
use std::convert::TryFrom;
use std::fmt::Debug;
use std::marker::PhantomData;

use derive_more::From;

use futures::{Stream, StreamExt, TryStreamExt};

use common::async_rpc::OpError;
use common::conn::BoxFuture;

use crypto::hash::{hash_buffer, Hasher};
use crypto::identity::compare_public_key;

use identity::IdentityClient;

use proto::app_server::messages::RelayAddress;
use proto::crypto::{HashResult, PublicKey, RandValue, Signature};
use proto::funder::messages::{
    CancelSendFundsOp, Currency, FriendTcOp, McBalance, MoveToken, RequestSendFundsOp,
    ResetBalance, ResetTerms, ResponseSendFundsOp, TokenInfo,
};

use signature::canonical::CanonicalSerialize;
use signature::signature_buff::{
    hash_token_info, move_token_signature_buff, reset_token_signature_buff,
};
use signature::verify::verify_move_token;

use database::transaction::{TransFunc, Transaction};

use crate::mutual_credit::incoming::{process_operation, IncomingMessage, ProcessOperationError};
use crate::mutual_credit::outgoing::{
    queue_cancel, queue_request, queue_response, QueueOperationError,
};
use crate::mutual_credit::types::{McCancel, McDbClient, McOp, McRequest, McResponse};

use crate::token_channel::types::{TcDbClient, TcStatus};
use crate::types::{create_hashed, MoveTokenHashed};

/// Unrecoverable TokenChannel error
#[derive(Debug, From)]
pub enum TokenChannelError {
    InvalidTransaction(ProcessOperationError),
    MoveTokenCounterOverflow,
    CanNotRemoveCurrencyInUse,
    InvalidTokenChannelStatus,
    RequestSignatureError,
    // TODO: Should we panic instead of returning InvalidState error?
    InvalidState,
    OpError(OpError),
    QueueOperationError(QueueOperationError),
}

#[derive(Debug)]
pub struct MoveTokenReceived {
    pub incoming_messages: Vec<(Currency, IncomingMessage)>,
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
pub enum ReceiveMoveTokenOutput {
    Duplicate,
    RetransmitOutgoing(MoveToken),
    Received(MoveTokenReceived),
    ChainInconsistent(ResetTerms),
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
pub fn initial_move_token(low_public_key: &PublicKey, high_public_key: &PublicKey) -> MoveToken {
    /*
    let token_info = TokenInfo {
        // No balances yet:
        balances_hash: hash_buffer(&[]),
        move_token_counter: 0,
    };

    let info_hash = hash_token_info(&low_public_key, &high_public_key, &token_info);
    */

    // This is a special initialization case.
    // Note that this is the only case where new_token is not a valid signature.
    // We do this because we want to have synchronization between the two sides of the token
    // channel, however, the remote side has no means of generating the signature (Because he
    // doesn't have the private key). Therefore we use a dummy new_token instead.
    let move_token_out = MoveToken {
        old_token: token_from_public_key(&low_public_key),
        operations: Vec::new(),
        new_token: token_from_public_key(&high_public_key),
    };

    move_token_out
}

/*
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
*/

// TODO: Possibly have this function as a method on the ResetBalance struct.
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
async fn local_balances_for_reset(
    tc_client: &mut impl TcDbClient,
) -> Result<HashMap<Currency, ResetBalance>, TokenChannelError> {
    let mut balances = HashMap::new();
    let mut reset_balances = tc_client.list_local_reset_balances();
    while let Some(item) = reset_balances.next().await {
        let (currency, reset_balance) = item?;
        if let Some(_) = balances.insert(currency, reset_balance) {
            return Err(TokenChannelError::InvalidState);
        }
    }
    Ok(balances)
}

struct InconsistentTrans<'a, C> {
    input_phantom: PhantomData<C>,
    move_token_out: MoveToken,
    new_move_token: MoveToken,
    local_public_key: &'a PublicKey,
    remote_public_key: &'a PublicKey,
}

enum TransactFail {
    TokenChannelError(TokenChannelError),
    InvalidIncoming(InvalidIncoming),
}

impl<'b, C> TransFunc for InconsistentTrans<'b, C>
where
    C: TcDbClient + Send,
    C::McDbClient: Send,
{
    type InRef = C;
    // We divide the error into two types:
    // Recoverable: InvalidIncoming
    // Unrecoverable: TokenChannelError
    type Out = Result<MoveTokenReceived, Result<InvalidIncoming, TokenChannelError>>;

    fn call<'a>(self, tc_client: &'a mut Self::InRef) -> BoxFuture<'a, Self::Out>
    where
        Self: 'a,
    {
        Box::pin(async move {
            let incoming_token_match_output = async move {
                tc_client
                    .set_outgoing_from_inconsistent(self.move_token_out.clone())
                    .await?;

                // Attempt to receive an incoming token:
                handle_incoming_token_match(
                    tc_client,
                    self.new_move_token,
                    self.local_public_key,
                    self.remote_public_key,
                )
                .await
            }
            .await
            .map_err(|e| Err(e))?;

            match incoming_token_match_output {
                IncomingTokenMatchOutput::MoveTokenReceived(move_token_received) => {
                    Ok(move_token_received)
                }
                IncomingTokenMatchOutput::InvalidIncoming(invalid_incoming) => {
                    Err(Ok(invalid_incoming))
                }
            }
        })
    }
}

pub async fn handle_in_move_token<C>(
    tc_client: &mut C,
    identity_client: &mut IdentityClient,
    new_move_token: MoveToken,
    local_public_key: &PublicKey,
    remote_public_key: &PublicKey,
) -> Result<ReceiveMoveTokenOutput, TokenChannelError>
where
    C: TcDbClient + Transaction + Send,
    C::McDbClient: Send,
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
        TcStatus::Inconsistent(local_reset_terms, _opt_remote_reset_terms) => {
            // Might be a reset move token
            if new_move_token.old_token == local_reset_terms.reset_token {
                // This is a reset move token!

                /*
                // Simulate an outgoing move token with the correct `new_token`:
                let token_info = TokenInfo {
                    balances_hash: hash_mc_infos(tc_client.list_local_reset_balances().map_ok(
                        |(currency, mc_balance)| {
                            (currency, reset_balance_to_mc_balance(mc_balance))
                        },
                    ))
                    .await?,
                    move_token_counter: local_reset_terms
                        .move_token_counter
                        .checked_sub(1)
                        .ok_or(TokenChannelError::MoveTokenCounterOverflow)?,
                };
                */

                let move_token_out = MoveToken {
                    old_token: Signature::from(&[0; Signature::len()]),
                    operations: Vec::new(),
                    // info_hash: hash_token_info(local_public_key, remote_public_key, &token_info),
                    new_token: local_reset_terms.reset_token.clone(),
                };

                // Atomically attempt to handle a reset move token:
                let output = tc_client
                    .transaction(InconsistentTrans {
                        input_phantom: PhantomData::<C>,
                        move_token_out,
                        new_move_token,
                        local_public_key,
                        remote_public_key,
                    })
                    .await;

                match output {
                    Ok(move_token_received) => {
                        Ok(ReceiveMoveTokenOutput::Received(move_token_received))
                    }
                    Err(e) => {
                        let invalid_incoming = e?;
                        // In this case the transaction was not committed:
                        Ok(ReceiveMoveTokenOutput::ChainInconsistent(ResetTerms {
                            reset_token: local_reset_terms.reset_token,
                            move_token_counter: local_reset_terms.move_token_counter,
                            reset_balances: local_balances_for_reset(tc_client).await?,
                        }))
                    }
                }
            } else {
                Ok(ReceiveMoveTokenOutput::ChainInconsistent(ResetTerms {
                    reset_token: local_reset_terms.reset_token,
                    move_token_counter: local_reset_terms.move_token_counter,
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
async fn set_inconsistent(
    tc_client: &mut impl TcDbClient,
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

async fn handle_in_move_token_dir_in(
    tc_client: &mut impl TcDbClient,
    identity_client: &mut IdentityClient,
    local_public_key: &PublicKey,
    remote_public_key: &PublicKey,
    move_token_in: MoveTokenHashed,
    new_move_token: MoveToken,
) -> Result<ReceiveMoveTokenOutput, TokenChannelError> {
    if move_token_in == create_hashed(&new_move_token, move_token_in.token_info.clone()) {
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

struct InMoveTokenDirOutTrans<'a, C> {
    input_phantom: PhantomData<C>,
    new_move_token: MoveToken,
    local_public_key: &'a PublicKey,
    remote_public_key: &'a PublicKey,
}

impl<'b, C> TransFunc for InMoveTokenDirOutTrans<'b, C>
where
    C: TcDbClient + Send,
    C::McDbClient: Send,
{
    type InRef = C;
    // We divide the error into two types:
    // Recoverable: InvalidIncoming
    // Unrecoverable: TokenChannelError
    type Out = Result<MoveTokenReceived, Result<InvalidIncoming, TokenChannelError>>;

    fn call<'a>(self, tc_client: &'a mut Self::InRef) -> BoxFuture<'a, Self::Out>
    where
        Self: 'a,
    {
        Box::pin(async move {
            let incoming_token_match_output = async move {
                // Attempt to receive an incoming token:
                handle_incoming_token_match(
                    tc_client,
                    self.new_move_token,
                    self.local_public_key,
                    self.remote_public_key,
                )
                .await
            }
            .await
            .map_err(|e| Err(e))?;

            match incoming_token_match_output {
                IncomingTokenMatchOutput::MoveTokenReceived(move_token_received) => {
                    Ok(move_token_received)
                }
                IncomingTokenMatchOutput::InvalidIncoming(invalid_incoming) => {
                    Err(Ok(invalid_incoming))
                }
            }
        })
    }
}

async fn handle_in_move_token_dir_out<C>(
    tc_client: &mut C,
    identity_client: &mut IdentityClient,
    move_token_out: MoveToken,
    new_move_token: MoveToken,
    local_public_key: &PublicKey,
    remote_public_key: &PublicKey,
) -> Result<ReceiveMoveTokenOutput, TokenChannelError>
where
    C: TcDbClient + Transaction + Send,
    C::McDbClient: Send,
{
    if new_move_token.old_token == move_token_out.new_token {
        // Atomically attempt to handle a reset move token:
        let output = tc_client
            .transaction(InMoveTokenDirOutTrans {
                input_phantom: PhantomData::<C>,
                new_move_token,
                local_public_key,
                remote_public_key,
            })
            .await;

        match output {
            Ok(move_token_received) => Ok(ReceiveMoveTokenOutput::Received(move_token_received)),
            Err(e) => {
                let invalid_incoming = e?;
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
    CanNotRemoveCurrencyInUse,
}

#[derive(Debug)]
enum IncomingTokenMatchOutput {
    MoveTokenReceived(MoveTokenReceived),
    InvalidIncoming(InvalidIncoming),
}

async fn handle_incoming_token_match(
    tc_client: &mut impl TcDbClient,
    new_move_token: MoveToken,
    local_public_key: &PublicKey,
    remote_public_key: &PublicKey,
) -> Result<IncomingTokenMatchOutput, TokenChannelError> {
    // Verify signature:
    // Note that we only verify the signature here, and not at the Incoming part.
    // This allows the genesis move token to occur smoothly, even though its signature
    // is not correct.
    // TODO: Check if the above statement is still true.

    // Aggregate incoming messages:
    let mut move_token_received = MoveTokenReceived {
        incoming_messages: Vec::new(),
    };

    // Attempt to apply operations for every currency:
    for friend_tc_op in new_move_token.operations.iter().cloned() {
        let tc_op =
            if let Some(tc_op) = tc_op_from_incoming_friend_tc_op(tc_client, friend_tc_op).await? {
                tc_op
            } else {
                return Ok(IncomingTokenMatchOutput::InvalidIncoming(
                    InvalidIncoming::InvalidOperation,
                ));
            };

        let remote_max_debt = if let Some(remote_max_debt) = tc_client
            .get_currency_config(tc_op.currency.clone())
            .await?
            .map(|currency_config| currency_config.remote_max_debt)
        {
            remote_max_debt
        } else {
            // Currency is not configured.
            match tc_op.mc_op {
                McOp::Request(mc_request) => {
                    // In case of a request, we cancel it:
                    move_token_received.incoming_messages.push((
                        tc_op.currency.clone(),
                        IncomingMessage::RequestCancel(mc_request),
                    ));
                    continue;
                }
                McOp::Response(..) | McOp::Cancel(..) => {
                    return Ok(IncomingTokenMatchOutput::InvalidIncoming(
                        InvalidIncoming::InvalidOperation,
                    ));
                }
            }
        };

        let mc_client =
            if let Some(mc_client) = tc_client.mc_db_client(tc_op.currency.clone()).await? {
                mc_client
            } else {
                match &tc_op.mc_op {
                    McOp::Request(..) => {
                        // An incoming request might allow opening a mutual credit, in case we have a matching
                        // currency configuration. Otherwise, the request should be cancelled.
                        tc_client.add_mutual_credit(tc_op.currency.clone()).await?;
                        tc_client
                            .mc_db_client(tc_op.currency.clone())
                            .await?
                            .ok_or(TokenChannelError::InvalidState)?
                    }
                    McOp::Response(..) | McOp::Cancel(..) => {
                        return Ok(IncomingTokenMatchOutput::InvalidIncoming(
                            InvalidIncoming::InvalidOperation,
                        ));
                    }
                }
            };

        let res = process_operation(
            mc_client,
            tc_op.mc_op,
            &tc_op.currency,
            remote_public_key,
            remote_max_debt,
        )
        .await;

        let incoming_message = match res {
            Ok(incoming_message) => incoming_message,
            Err(_) => {
                return Ok(IncomingTokenMatchOutput::InvalidIncoming(
                    InvalidIncoming::InvalidOperation,
                ))
            }
        };

        move_token_received
            .incoming_messages
            .push((tc_op.currency.clone(), incoming_message));
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

    // Verify signature:
    if !verify_move_token(&new_move_token, &info_hash, &remote_public_key) {
        return Ok(IncomingTokenMatchOutput::InvalidIncoming(
            InvalidIncoming::InvalidSignature,
        ));
    }

    // Set direction to outgoing, with the newly received move token:
    let new_move_token_hashed = create_hashed(&new_move_token, token_info);
    tc_client
        .set_direction_incoming(new_move_token_hashed)
        .await?;

    Ok(IncomingTokenMatchOutput::MoveTokenReceived(
        move_token_received,
    ))
}

#[derive(Debug, Clone)]
pub struct TcOp {
    pub currency: Currency,
    pub mc_op: McOp,
}

async fn tc_op_from_incoming_friend_tc_op(
    tc_client: &mut impl TcDbClient,
    friend_tc_op: FriendTcOp,
) -> Result<Option<TcOp>, TokenChannelError> {
    Ok(Some(match friend_tc_op {
        FriendTcOp::RequestSendFunds(request_send_funds) => TcOp {
            currency: request_send_funds.currency,
            mc_op: McOp::Request(McRequest {
                request_id: request_send_funds.request_id,
                src_hashed_lock: request_send_funds.src_hashed_lock,
                dest_payment: request_send_funds.dest_payment,
                invoice_hash: request_send_funds.invoice_hash,
                route: request_send_funds.route,
                left_fees: request_send_funds.left_fees,
            }),
        },
        FriendTcOp::ResponseSendFunds(response_send_funds) => {
            let currency = if let Some(currency) = tc_client
                .get_currency_local_request(response_send_funds.request_id.clone())
                .await?
            {
                currency
            } else {
                return Ok(None);
            };

            TcOp {
                currency,
                mc_op: McOp::Response(McResponse {
                    request_id: response_send_funds.request_id,
                    src_plain_lock: response_send_funds.src_plain_lock,
                    serial_num: response_send_funds.serial_num,
                    signature: response_send_funds.signature,
                }),
            }
        }
        FriendTcOp::CancelSendFunds(cancel_send_funds) => {
            let currency = if let Some(currency) = tc_client
                .get_currency_local_request(cancel_send_funds.request_id.clone())
                .await?
            {
                currency
            } else {
                return Ok(None);
            };

            TcOp {
                currency,
                mc_op: McOp::Cancel(McCancel {
                    request_id: cancel_send_funds.request_id,
                }),
            }
        }
    }))
}

fn friend_tc_op_from_outgoing_tc_op(tc_op: TcOp) -> FriendTcOp {
    match tc_op.mc_op {
        McOp::Request(mc_request) => FriendTcOp::RequestSendFunds(RequestSendFundsOp {
            request_id: mc_request.request_id,
            currency: tc_op.currency,
            src_hashed_lock: mc_request.src_hashed_lock,
            dest_payment: mc_request.dest_payment,
            invoice_hash: mc_request.invoice_hash,
            route: mc_request.route,
            left_fees: mc_request.left_fees,
        }),
        McOp::Response(mc_response) => FriendTcOp::ResponseSendFunds(ResponseSendFundsOp {
            request_id: mc_response.request_id,
            src_plain_lock: mc_response.src_plain_lock,
            serial_num: mc_response.serial_num,
            signature: mc_response.signature,
        }),
        McOp::Cancel(mc_cancel) => FriendTcOp::CancelSendFunds(CancelSendFundsOp {
            request_id: mc_cancel.request_id,
        }),
    }
}

pub struct OutMoveToken {
    tc_ops: Vec<TcOp>,
}

impl OutMoveToken {
    pub fn new() -> Self {
        Self { tc_ops: Vec::new() }
    }

    pub async fn queue_request(
        &mut self,
        tc_client: &mut impl TcDbClient,
        currency: Currency,
        mc_request: McRequest,
    ) -> Result<Result<McBalance, McCancel>, TokenChannelError> {
        // Token channel must be at a consistent incoming state
        assert!(matches!(
            tc_client.get_tc_status().await?,
            TcStatus::ConsistentIn(..)
        ));

        let local_max_debt = tc_client
            .get_currency_config(currency.clone())
            .await?
            .ok_or(TokenChannelError::InvalidState)?
            .local_max_debt;
        let mc_client = if let Some(mc_client) = tc_client.mc_db_client(currency.clone()).await? {
            mc_client
        } else {
            // We need to create a new mutual credit
            tc_client.add_mutual_credit(currency.clone()).await?;
            tc_client
                .mc_db_client(currency.clone())
                .await?
                .ok_or(TokenChannelError::InvalidState)?
        };

        // queue_request might fail due to `local_max_debt`.
        if let Err(mc_cancel) =
            queue_request(mc_client, mc_request.clone(), &currency, local_max_debt).await?
        {
            return Ok(Err(mc_cancel));
        }

        self.tc_ops.push(TcOp {
            currency: currency.clone(),
            mc_op: McOp::Request(mc_request),
        });

        // Get resulting balances (After queue-ing the request):
        Ok(Ok(mc_client.get_balance().await?))
    }

    pub async fn queue_response(
        &mut self,
        tc_client: &mut impl TcDbClient,
        currency: Currency,
        mc_response: McResponse,
        local_public_key: &PublicKey,
    ) -> Result<McBalance, TokenChannelError> {
        // Token channel must be at a consistent incoming state
        assert!(matches!(
            tc_client.get_tc_status().await?,
            TcStatus::ConsistentIn(..)
        ));

        let mc_client = tc_client
            .mc_db_client(currency.clone())
            .await?
            .ok_or(TokenChannelError::InvalidState)?;

        queue_response(mc_client, mc_response.clone(), local_public_key).await?;

        self.tc_ops.push(TcOp {
            currency,
            mc_op: McOp::Response(mc_response),
        });

        Ok(mc_client.get_balance().await?)
    }

    pub async fn queue_cancel(
        &mut self,
        tc_client: &mut impl TcDbClient,
        currency: Currency,
        mc_cancel: McCancel,
    ) -> Result<McBalance, TokenChannelError> {
        // Token channel must be at a consistent incoming state
        assert!(matches!(
            tc_client.get_tc_status().await?,
            TcStatus::ConsistentIn(..)
        ));

        let mc_client = tc_client
            .mc_db_client(currency.clone())
            .await?
            .ok_or(TokenChannelError::InvalidState)?;
        queue_cancel(mc_client, mc_cancel.clone()).await?;

        self.tc_ops.push(TcOp {
            currency,
            mc_op: McOp::Cancel(mc_cancel),
        });

        Ok(mc_client.get_balance().await?)
    }

    pub async fn finalize(
        self,
        tc_client: &mut impl TcDbClient,
        identity_client: &mut IdentityClient,
        local_public_key: &PublicKey,
        remote_public_key: &PublicKey,
    ) -> Result<MoveToken, TokenChannelError> {
        // Token channel must be at a consistent incoming state
        assert!(matches!(
            tc_client.get_tc_status().await?,
            TcStatus::ConsistentIn(..)
        ));

        let move_token_in = match tc_client.get_tc_status().await? {
            TcStatus::ConsistentIn(move_token_in) => move_token_in,
            TcStatus::ConsistentOut(..) | TcStatus::Inconsistent(..) => {
                return Err(TokenChannelError::InvalidTokenChannelStatus);
            }
        };

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

        let info_hash = hash_token_info(local_public_key, remote_public_key, &token_info);

        let mut move_token = MoveToken {
            old_token: move_token_in.new_token,
            operations: self
                .tc_ops
                .into_iter()
                .map(|tc_op| friend_tc_op_from_outgoing_tc_op(tc_op))
                .collect(),
            // Still not known:
            new_token: Signature::from(&[0; Signature::len()]),
        };

        // Fill in signature:
        let signature_buff = move_token_signature_buff(&move_token, &info_hash);
        move_token.new_token = identity_client
            .request_signature(signature_buff)
            .await
            .map_err(|_| TokenChannelError::RequestSignatureError)?;

        // Set the direction to be outgoing, and save last incoming token in an atomic way:
        tc_client
            .set_direction_outgoing(move_token.clone(), new_move_token_counter)
            .await?;

        Ok(move_token)
    }
}

/// Apply a token channel reset, accepting remote side's
/// reset terms.
pub async fn accept_remote_reset(
    tc_client: &mut impl TcDbClient,
    identity_client: &mut IdentityClient,
    operations: Vec<FriendTcOp>,
    local_public_key: &PublicKey,
    remote_public_key: &PublicKey,
) -> Result<MoveToken, TokenChannelError> {
    // Make sure that we are in an inconsistent state,
    // and that the remote side has already sent his reset terms:
    let (remote_reset_token, remote_reset_move_token_counter) =
        match tc_client.get_tc_status().await? {
            TcStatus::ConsistentIn(..)
            | TcStatus::ConsistentOut(..)
            | TcStatus::Inconsistent(_, None) => {
                // We don't have the remote side's reset terms yet:
                return Err(TokenChannelError::InvalidTokenChannelStatus);
            }
            TcStatus::Inconsistent(_local_reset_terms, Some(remote_reset_terms)) => (
                remote_reset_terms.reset_token,
                remote_reset_terms.move_token_counter,
            ),
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

    let move_token_in = MoveToken {
        old_token: Signature::from(&[0; Signature::len()]),
        operations: Vec::new(),
        new_token: remote_reset_token.clone(),
    };

    tc_client
        .set_incoming_from_inconsistent(create_hashed(&move_token_in, token_info))
        .await?;

    // Create an outgoing move token, to be sent to the remote side:
    OutMoveToken::new()
        .finalize(
            tc_client,
            identity_client,
            local_public_key,
            remote_public_key,
        )
        .await
}

/// Load the remote reset terms information from a remote side's inconsistency message
/// Optionally returns local reset terms (If not already inconsistent)
pub async fn load_remote_reset_terms(
    tc_client: &mut impl TcDbClient,
    identity_client: &mut IdentityClient,
    remote_reset_terms: ResetTerms,
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
        .set_inconsistent_remote_terms(
            remote_reset_terms.reset_token,
            remote_reset_terms.move_token_counter,
        )
        .await?;

    for (currency, mc_balance) in remote_reset_terms.reset_balances.into_iter() {
        tc_client
            .add_remote_reset_balance(currency, mc_balance)
            .await?;
    }

    Ok(ret_val)
}
