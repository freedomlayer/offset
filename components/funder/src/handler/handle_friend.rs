use signature::canonical::CanonicalSerialize;
use std::fmt::Debug;

use crypto::rand::{CryptoRandom, RandGen};

use proto::crypto::{PublicKey, Signature, Uid};

use proto::app_server::messages::RelayAddress;
use proto::funder::messages::{
    BalanceInfo, CancelSendFundsOp, ChannelerUpdateFriend, CollectSendFundsOp, CountersInfo,
    Currency, CurrencyBalance, CurrencyBalanceInfo, FriendMessage, FunderOutgoingControl, McInfo,
    MoveTokenRequest, PaymentStatus, PaymentStatusSuccess, PendingTransaction, RequestResult,
    RequestSendFundsOp, ResetTerms, ResponseClosePayment, ResponseSendFundsOp, TokenInfo,
    TransactionResult,
};
use signature::signature_buff::hash_token_info;
use signature::verify::verify_move_token;

use crate::mutual_credit::incoming::{
    IncomingCancelSendFundsOp, IncomingCollectSendFundsOp, IncomingMessage,
    IncomingResponseSendFundsOp,
};
use crate::token_channel::{MoveTokenReceived, ReceiveMoveTokenOutput, TokenChannel};

use crate::types::{create_pending_transaction, ChannelerConfig};

use crate::friend::{
    BackwardsOp, ChannelInconsistent, ChannelStatus, CurrencyConfig, FriendMutation,
    SentLocalRelays,
};
use crate::state::{FunderMutation, FunderState, Payment, PaymentStage};

use crate::ephemeral::Ephemeral;

use crate::handler::canceler::{
    cancel_local_pending_transactions, cancel_pending_requests, remove_transaction,
    reply_with_cancel, CurrencyChoice,
};
use crate::handler::prepare::{prepare_commit, prepare_receipt};
use crate::handler::state_wrap::{MutableEphemeral, MutableFunderState};
use crate::handler::types::SendCommands;
use crate::handler::utils::{
    find_remote_pending_transaction, find_request_origin, is_friend_ready,
};

#[derive(Debug)]
pub enum HandleFriendError {
    FriendDoesNotExist,
    InconsistencyWhenTokenOwned,
}

/// Generate a random token to be used for resetting the channel.
fn gen_channel_reset_token<R>(rng: &R) -> Signature
where
    R: CryptoRandom,
{
    let mut buff = [0; Signature::len()];
    rng.fill(&mut buff).unwrap();
    Signature::from(&buff)
}

pub fn gen_reset_terms<B, R>(token_channel: &TokenChannel<B>, rng: &R) -> ResetTerms
where
    R: CryptoRandom,
    B: Clone + PartialEq + Eq + CanonicalSerialize + Debug,
{
    // We add 2 for the new counter in case
    // the remote side has already used the next counter.
    let reset_token = gen_channel_reset_token(rng);

    let mut mutual_credits: Vec<_> = token_channel.get_mutual_credits().iter().collect();
    mutual_credits.sort_by(|(cur_a, _), (cur_b, _)| cur_a.cmp(cur_b));
    let balance_for_reset = mutual_credits
        .into_iter()
        .map(|(currency, mutual_credit)| CurrencyBalance {
            currency: currency.clone(),
            balance: mutual_credit.balance_for_reset(),
        })
        .collect();

    ResetTerms {
        reset_token,
        // TODO: Should we do something other than wrapping_add(1)?
        // 2**64 inconsistencies are required for an overflow.
        inconsistency_counter: token_channel.get_inconsistency_counter().wrapping_add(1),
        balance_for_reset,
    }
}

/// Check if channel reset is required (Remote side used the RESET token)
/// If so, reset the channel.
pub fn try_reset_channel<B>(
    m_state: &mut MutableFunderState<B>,
    send_commands: &mut SendCommands,
    friend_public_key: &PublicKey,
    local_reset_terms: &ResetTerms,
    move_token_request: &MoveTokenRequest<B>,
) where
    B: Clone + PartialEq + Eq + CanonicalSerialize + Debug,
{
    let move_token = &move_token_request.move_token;

    let balances_for_reset = local_reset_terms
        .balance_for_reset
        .iter()
        .map(|currency_balance| CurrencyBalanceInfo {
            currency: currency_balance.currency.clone(),
            balance_info: BalanceInfo {
                balance: currency_balance.balance.checked_neg().unwrap(),
                local_pending_debt: 0,
                remote_pending_debt: 0,
            },
        })
        .collect();

    // Obtain token channel, and get current
    let remote_token_info = TokenInfo {
        mc: McInfo {
            local_public_key: friend_public_key.clone(),
            remote_public_key: m_state.state().local_public_key.clone(),
            balances: balances_for_reset,
        },
        counters: CountersInfo {
            inconsistency_counter: local_reset_terms.inconsistency_counter,
            move_token_counter: 0,
        },
    };

    // Check if incoming message is a valid attempt to reset the channel:
    if move_token.old_token != local_reset_terms.reset_token
        || !move_token.currencies_operations.is_empty()
        || hash_token_info(&remote_token_info) != move_token.info_hash
        || !verify_move_token(move_token, friend_public_key)
    {
        send_commands.set_resend_outgoing(friend_public_key);
        return;
    }

    let token_channel = TokenChannel::new_from_remote_reset(move_token, &remote_token_info);

    // This is a reset message. We reset the token channel:
    let friend_mutation = FriendMutation::SetConsistent(token_channel);
    let funder_mutation =
        FunderMutation::FriendMutation((friend_public_key.clone(), friend_mutation));
    m_state.mutate(funder_mutation);

    // Update our configured currencies accordingly (Possibly add new currencies with empty
    // configurations):
    let currency_configs = m_state
        .state()
        .friends
        .get(friend_public_key)
        .unwrap()
        .currency_configs
        .clone();
    for currency_balance in &local_reset_terms.balance_for_reset {
        if !currency_configs.contains_key(&currency_balance.currency) {
            // This is a reset message. We reset the token channel:
            let friend_mutation = FriendMutation::UpdateCurrencyConfig((
                currency_balance.currency.clone(),
                CurrencyConfig::new(),
            ));
            let funder_mutation =
                FunderMutation::FriendMutation((friend_public_key.clone(), friend_mutation));
            m_state.mutate(funder_mutation);
        }
    }

    // Update our knowledge about remote relays if required:
    if let Some(remote_relays) = &move_token.opt_local_relays {
        if remote_relays
            != &m_state
                .state()
                .friends
                .get(&friend_public_key)
                .unwrap()
                .remote_relays
        {
            let friend_mutation = FriendMutation::SetRemoteRelays(remote_relays.clone());
            let funder_mutation =
                FunderMutation::FriendMutation((friend_public_key.clone(), friend_mutation));
            m_state.mutate(funder_mutation);
        }
    }

    // We should send our relays to the remote side:
    send_commands.set_resend_relays(friend_public_key);

    send_commands.set_try_send(friend_public_key);
    if move_token_request.token_wanted {
        send_commands.set_remote_wants_token(friend_public_key);
    }
}

/// Forward a request message to the relevant friend and token channel.
fn forward_request<B>(
    m_state: &mut MutableFunderState<B>,
    send_commands: &mut SendCommands,
    currency: &Currency,
    request_send_funds: RequestSendFundsOp,
    next_pk: &PublicKey,
) where
    B: Clone + PartialEq + Eq + CanonicalSerialize + Debug,
{
    // Queue message to the relevant friend. Later this message will be queued to a specific
    // available token channel:
    let friend_mutation =
        FriendMutation::PushBackPendingRequest((currency.clone(), request_send_funds.clone()));
    let funder_mutation = FunderMutation::FriendMutation((next_pk.clone(), friend_mutation));
    m_state.mutate(funder_mutation);
    send_commands.set_try_send(next_pk);
}

#[derive(Debug)]
enum CheckRequest {
    Complete,
    Success,
    Failure,
}

/// Check if we can add a request into a local OpenInvoice
fn check_request<B>(state: &FunderState<B>, request_send_funds: &RequestSendFundsOp) -> CheckRequest
where
    B: Clone + PartialEq + Eq + CanonicalSerialize + Debug,
{
    // First make sure that we have a matching open invoice for this transaction:
    let open_invoice =
        if let Some(open_invoice) = state.open_invoices.get(&request_send_funds.invoice_id) {
            open_invoice
        } else {
            return CheckRequest::Failure;
        };

    if let Some(src_hashed_lock) = &open_invoice.opt_src_hashed_lock {
        if src_hashed_lock != &request_send_funds.src_hashed_lock {
            return CheckRequest::Failure;
        }
    }

    if open_invoice.total_dest_payment != request_send_funds.total_dest_payment {
        return CheckRequest::Failure;
    }
    if request_send_funds.dest_payment > request_send_funds.total_dest_payment {
        return CheckRequest::Failure;
    }

    // Calculate the amounts of funds already paid for this OpenInvoice:
    let mut total_paid = 0u128;
    for request_id in &open_invoice.incoming_transactions {
        let pending_transaction =
            find_remote_pending_transaction(state, &open_invoice.currency, request_id).unwrap();
        assert_eq!(
            pending_transaction.total_dest_payment,
            open_invoice.total_dest_payment
        );
        total_paid = total_paid
            .checked_add(pending_transaction.dest_payment)
            .unwrap();
    }

    // Check if we have room to pay more with the new transaction:
    let new_total_paid =
        if let Some(new_total_paid) = total_paid.checked_add(request_send_funds.dest_payment) {
            new_total_paid
        } else {
            // Overflow occured:
            return CheckRequest::Failure;
        };

    if new_total_paid < open_invoice.total_dest_payment {
        // Request is allowed, the invoice is not fully paid:
        CheckRequest::Success
    } else if new_total_paid == open_invoice.total_dest_payment {
        // Request is allowed, and the invoice is fully paid:
        CheckRequest::Complete
    } else {
        // We do not allow paying more than total_dest_payment
        CheckRequest::Failure
    }
}

fn handle_request_send_funds<B>(
    m_state: &mut MutableFunderState<B>,
    ephemeral: &Ephemeral,
    send_commands: &mut SendCommands,
    remote_public_key: &PublicKey,
    currency: &Currency,
    mut request_send_funds: RequestSendFundsOp,
) where
    B: Clone + PartialEq + Eq + CanonicalSerialize + Debug,
{
    if request_send_funds.route.is_empty() {
        // We are the destination of this request.

        let is_complete = match check_request(m_state.state(), &request_send_funds) {
            CheckRequest::Failure => {
                reply_with_cancel(
                    m_state,
                    send_commands,
                    remote_public_key,
                    currency,
                    &request_send_funds.request_id,
                );
                return;
            }
            CheckRequest::Success => false,
            CheckRequest::Complete => true,
        };

        // Set the src_hashed_lock for the OpenInvoice if required.
        // (Happens only when the first transaction for this invoice is received):
        if m_state
            .state()
            .open_invoices
            .get(&request_send_funds.invoice_id)
            .unwrap()
            .opt_src_hashed_lock
            .is_none()
        {
            // Set the src_hashed_lock accordingly:
            let funder_mutation = FunderMutation::SetInvoiceSrcHashedLock((
                request_send_funds.invoice_id.clone(),
                request_send_funds.src_hashed_lock.clone(),
            ));
            m_state.mutate(funder_mutation);
        }

        // Add the incoming transaction:
        let funder_mutation = FunderMutation::AddIncomingTransaction((
            request_send_funds.invoice_id.clone(),
            request_send_funds.request_id.clone(),
        ));
        m_state.mutate(funder_mutation);

        // We return a response:
        let pending_transaction = create_pending_transaction(&request_send_funds);
        m_state.queue_unsigned_response(
            remote_public_key.clone(),
            currency.clone(),
            pending_transaction,
            is_complete,
        );
        send_commands.set_try_send(&remote_public_key);
        return;
    }

    // We are not the destination of this request.
    // The node on the route has to be one of our friends:
    let next_public_key = request_send_funds.route.index_to_pk(0).unwrap().clone();
    let friend_exists = m_state.state().friends.contains_key(&next_public_key);

    // This friend must be considered online for us to forward the message.
    // If we forward the request to an offline friend, the request could be stuck for a long
    // time before a response arrives.
    let friend_ready = if friend_exists {
        is_friend_ready(m_state.state(), ephemeral, &next_public_key, currency)
    } else {
        false
    };

    // Attempt to take our fee for forwarding the request.
    // Note that the rate is determined by the rate we set with the node that sent us the request
    // (And **not** with the node that we forward the request to).
    let currency_configs = &m_state
        .state()
        .friends
        .get(remote_public_key)
        .unwrap()
        .currency_configs;

    // Get the rate for this currency.
    // // If not set up, the rate is 0:
    // // let default_rate = Rate::new();
    let rate = currency_configs.get(currency).unwrap().rate.clone();

    let opt_local_fee = rate.calc_fee(request_send_funds.dest_payment);

    let request_id = request_send_funds.request_id.clone();

    // Make sure that calc_fee() worked, and that we can take this amount of credits:
    let opt_request_send_funds = if let Some(local_fee) = opt_local_fee {
        if let Some(new_left_fees) = request_send_funds.left_fees.checked_sub(local_fee) {
            request_send_funds.left_fees = new_left_fees;
            Some(request_send_funds)
        } else {
            None
        }
    } else {
        None
    };

    let mut request_send_funds = match (opt_request_send_funds, friend_ready) {
        (Some(request_send_funds), true) => request_send_funds,
        _ => {
            reply_with_cancel(
                m_state,
                send_commands,
                remote_public_key,
                currency,
                &request_id,
            );
            return;
        }
    };

    // Remove the next node from remaining route.
    request_send_funds.route.public_keys.remove(0);

    // Queue message to the next node.
    forward_request(
        m_state,
        send_commands,
        currency,
        request_send_funds,
        &next_public_key,
    );
}

fn handle_response_send_funds<B>(
    m_state: &mut MutableFunderState<B>,
    send_commands: &mut SendCommands,
    outgoing_control: &mut Vec<FunderOutgoingControl<B>>,
    currency: &Currency,
    response_send_funds: ResponseSendFundsOp,
    pending_transaction: PendingTransaction,
) where
    B: Clone + PartialEq + Eq + CanonicalSerialize + Debug,
{
    match find_request_origin(m_state.state(), currency, &response_send_funds.request_id).cloned() {
        None => {
            // We couldn't find any external origin.
            // It means that we are the origin of this request

            // Keep the response:
            let funder_mutation =
                FunderMutation::SetTransactionResponse(response_send_funds.clone());
            m_state.mutate(funder_mutation);

            let payment_id = m_state
                .state()
                .open_transactions
                .get(&response_send_funds.request_id)
                .unwrap()
                .payment_id
                .clone();

            let payment = m_state.state().payments.get(&payment_id).unwrap();
            let transaction_result = if response_send_funds.is_complete {
                let commit = prepare_commit(
                    currency.clone(),
                    &response_send_funds,
                    &pending_transaction,
                    payment.src_plain_lock.clone(),
                );

                TransactionResult {
                    request_id: response_send_funds.request_id.clone(),
                    result: RequestResult::Complete(commit),
                }
            } else {
                TransactionResult {
                    request_id: response_send_funds.request_id.clone(),
                    result: RequestResult::Success,
                }
            };
            outgoing_control.push(FunderOutgoingControl::TransactionResult(transaction_result));
        }
        Some(friend_public_key) => {
            // Queue this response message to another token channel:
            let response_op = BackwardsOp::Response(response_send_funds);
            let friend_mutation =
                FriendMutation::PushBackPendingBackwardsOp((currency.clone(), response_op));
            let funder_mutation =
                FunderMutation::FriendMutation((friend_public_key.clone(), friend_mutation));
            m_state.mutate(funder_mutation);

            send_commands.set_try_send(&friend_public_key);
        }
    }
}

fn handle_cancel_send_funds<B, R>(
    m_state: &mut MutableFunderState<B>,
    send_commands: &mut SendCommands,
    outgoing_control: &mut Vec<FunderOutgoingControl<B>>,
    rng: &R,
    currency: &Currency,
    cancel_send_funds: CancelSendFundsOp,
    pending_transaction: PendingTransaction,
) where
    B: Clone + PartialEq + Eq + CanonicalSerialize + Debug,
    R: CryptoRandom,
{
    match find_request_origin(m_state.state(), currency, &cancel_send_funds.request_id).cloned() {
        None => {
            // We are the origin of this request, and we got a cancellation.

            // Update buyer transactions (requests that were originated by us):
            remove_transaction(
                m_state,
                outgoing_control,
                rng,
                &cancel_send_funds.request_id,
            );

            // Inform user about the transaction failure:
            outgoing_control.push(FunderOutgoingControl::TransactionResult(
                TransactionResult {
                    request_id: pending_transaction.request_id,
                    result: RequestResult::Failure,
                },
            ));
        }
        Some(friend_public_key) => {
            // Queue this Cancel message to another token channel:
            let cancel_op = BackwardsOp::Cancel(cancel_send_funds);
            let friend_mutation =
                FriendMutation::PushBackPendingBackwardsOp((currency.clone(), cancel_op));
            let funder_mutation =
                FunderMutation::FriendMutation((friend_public_key.clone(), friend_mutation));
            m_state.mutate(funder_mutation);

            send_commands.set_try_send(&friend_public_key);
        }
    };
}

fn handle_collect_send_funds<B, R>(
    m_state: &mut MutableFunderState<B>,
    send_commands: &mut SendCommands,
    outgoing_control: &mut Vec<FunderOutgoingControl<B>>,
    rng: &R,
    currency: &Currency,
    collect_send_funds: CollectSendFundsOp,
    pending_transaction: PendingTransaction,
) where
    B: Clone + PartialEq + Eq + CanonicalSerialize + Debug,
    R: CryptoRandom,
{
    // Check if we are the origin of this transaction (Did we send the RequestSendFundsOp
    // message?):
    match find_request_origin(m_state.state(), currency, &collect_send_funds.request_id).cloned() {
        None => {
            // We are the origin of this request, and we got a Collect message
            let open_transaction = m_state
                .state()
                .open_transactions
                .get(&collect_send_funds.request_id)
                .unwrap();

            let payment = m_state
                .state()
                .payments
                .get(&open_transaction.payment_id)
                .unwrap();

            // Update payment status:
            let (opt_new_payment_stage, opt_payment_status) = match &payment.stage {
                PaymentStage::NewTransactions(new_transactions) => {
                    // Create a Receipt:
                    let receipt = prepare_receipt(
                        currency,
                        &collect_send_funds,
                        open_transaction.opt_response.as_ref().unwrap(),
                        &pending_transaction,
                    );
                    let ack_uid = Uid::rand_gen(rng);
                    (
                        Some(PaymentStage::Success((
                            new_transactions.num_transactions.checked_sub(1).unwrap(),
                            receipt.clone(),
                            ack_uid.clone(),
                        ))),
                        Some(PaymentStatus::Success(PaymentStatusSuccess {
                            receipt,
                            ack_uid,
                        })),
                    )
                }
                PaymentStage::InProgress(num_transactions) => {
                    assert!(*num_transactions > 0);
                    // Create a Receipt:
                    let receipt = prepare_receipt(
                        currency,
                        &collect_send_funds,
                        open_transaction.opt_response.as_ref().unwrap(),
                        &pending_transaction,
                    );
                    let ack_uid = Uid::rand_gen(rng);
                    (
                        Some(PaymentStage::Success((
                            num_transactions.checked_sub(1).unwrap(),
                            receipt.clone(),
                            ack_uid.clone(),
                        ))),
                        Some(PaymentStatus::Success(PaymentStatusSuccess {
                            receipt,
                            ack_uid,
                        })),
                    )
                }
                PaymentStage::Success((num_transactions, receipt, ack_uid)) => (
                    Some(PaymentStage::Success((
                        num_transactions.checked_sub(1).unwrap(),
                        receipt.clone(),
                        ack_uid.clone(),
                    ))),
                    Some(PaymentStatus::Success(PaymentStatusSuccess {
                        receipt: receipt.clone(),
                        ack_uid: ack_uid.clone(),
                    })),
                ),
                PaymentStage::Canceled(_) => unreachable!(),
                PaymentStage::AfterSuccessAck(num_transactions) => {
                    let new_num_transactions = num_transactions.checked_sub(1).unwrap();
                    (
                        if new_num_transactions > 0 {
                            Some(PaymentStage::AfterSuccessAck(new_num_transactions))
                        } else {
                            None
                        },
                        None,
                    )
                }
            };

            // Possibly send back a ResponseClosePayment:
            if let Some(payment_status) = opt_payment_status {
                let response_close_payment = ResponseClosePayment {
                    payment_id: open_transaction.payment_id.clone(),
                    status: payment_status,
                };
                outgoing_control.push(FunderOutgoingControl::ResponseClosePayment(
                    response_close_payment,
                ));
            }

            let funder_mutation = if let Some(new_payment_stage) = opt_new_payment_stage {
                // Update payment:
                let new_payment = Payment {
                    src_plain_lock: payment.src_plain_lock.clone(),
                    stage: new_payment_stage,
                };
                FunderMutation::UpdatePayment((open_transaction.payment_id.clone(), new_payment))
            } else {
                FunderMutation::RemovePayment(open_transaction.payment_id.clone())
                // Remove payment:
            };
            m_state.mutate(funder_mutation);

            // Remove transaction:
            let funder_mutation =
                FunderMutation::RemoveTransaction(collect_send_funds.request_id.clone());
            m_state.mutate(funder_mutation);
        }
        Some(friend_public_key) => {
            // Queue this Collect message to another token channel:
            let collect_op = BackwardsOp::Collect(collect_send_funds);
            let friend_mutation =
                FriendMutation::PushBackPendingBackwardsOp((currency.clone(), collect_op));
            let funder_mutation =
                FunderMutation::FriendMutation((friend_public_key.clone(), friend_mutation));
            m_state.mutate(funder_mutation);

            send_commands.set_try_send(&friend_public_key);
        }
    };
}

/// Process valid incoming operations from remote side.
fn handle_move_token_output<B, R>(
    m_state: &mut MutableFunderState<B>,
    m_ephemeral: &mut MutableEphemeral,
    send_commands: &mut SendCommands,
    outgoing_control: &mut Vec<FunderOutgoingControl<B>>,
    rng: &R,
    remote_public_key: &PublicKey,
    currency: &Currency,
    incoming_messages: Vec<IncomingMessage>,
) where
    B: Clone + PartialEq + Eq + CanonicalSerialize + Debug,
    R: CryptoRandom,
{
    for incoming_message in incoming_messages {
        match incoming_message {
            IncomingMessage::Request(request_send_funds) => {
                handle_request_send_funds(
                    m_state,
                    m_ephemeral.ephemeral(),
                    send_commands,
                    remote_public_key,
                    currency,
                    request_send_funds,
                );
            }
            IncomingMessage::Response(IncomingResponseSendFundsOp {
                pending_transaction,
                incoming_response,
            }) => {
                handle_response_send_funds(
                    m_state,
                    send_commands,
                    outgoing_control,
                    currency,
                    incoming_response,
                    pending_transaction,
                );
            }
            IncomingMessage::Cancel(IncomingCancelSendFundsOp {
                pending_transaction,
                incoming_cancel,
            }) => {
                handle_cancel_send_funds(
                    m_state,
                    send_commands,
                    outgoing_control,
                    rng,
                    currency,
                    incoming_cancel,
                    pending_transaction,
                );
            }
            IncomingMessage::Collect(IncomingCollectSendFundsOp {
                pending_transaction,
                incoming_collect,
            }) => {
                handle_collect_send_funds(
                    m_state,
                    send_commands,
                    outgoing_control,
                    rng,
                    currency,
                    incoming_collect,
                    pending_transaction,
                );
            }
        }
    }
}

/// Handle an error with incoming move token.
fn handle_move_token_error<B, R>(
    m_state: &mut MutableFunderState<B>,
    send_commands: &mut SendCommands,
    outgoing_control: &mut Vec<FunderOutgoingControl<B>>,
    rng: &R,
    remote_public_key: &PublicKey,
) where
    B: Clone + PartialEq + Eq + CanonicalSerialize + Debug,
    R: CryptoRandom,
{
    let friend = m_state.state().friends.get(remote_public_key).unwrap();
    let token_channel = match &friend.channel_status {
        ChannelStatus::Consistent(channel_consistent) => &channel_consistent.token_channel,
        ChannelStatus::Inconsistent(_) => unreachable!(),
    };
    let opt_last_incoming_move_token = token_channel.get_last_incoming_move_token_hashed().cloned();
    // Send an InconsistencyError message to remote side:
    let local_reset_terms = gen_reset_terms(&token_channel, rng);

    // Cancel all internal pending requests inside token channel:
    cancel_local_pending_transactions(
        m_state,
        send_commands,
        outgoing_control,
        rng,
        remote_public_key,
    );

    // Cancel all pending requests to this friend:
    cancel_pending_requests(
        m_state,
        send_commands,
        outgoing_control,
        rng,
        remote_public_key,
        &CurrencyChoice::All,
    );

    // Keep outgoing InconsistencyError message details in memory:
    let channel_inconsistent = ChannelInconsistent {
        opt_last_incoming_move_token,
        local_reset_terms,
        opt_remote_reset_terms: None,
    };
    let friend_mutation = FriendMutation::SetInconsistent(channel_inconsistent);
    let funder_mutation =
        FunderMutation::FriendMutation((remote_public_key.clone(), friend_mutation));
    m_state.mutate(funder_mutation);
    send_commands.set_try_send(remote_public_key);
}

/// Handle success with incoming move token.
fn handle_move_token_success<B, R>(
    m_state: &mut MutableFunderState<B>,
    m_ephemeral: &mut MutableEphemeral,
    send_commands: &mut SendCommands,
    outgoing_control: &mut Vec<FunderOutgoingControl<B>>,
    outgoing_channeler_config: &mut Vec<ChannelerConfig<RelayAddress<B>>>,
    rng: &R,
    remote_public_key: &PublicKey,
    receive_move_token_output: ReceiveMoveTokenOutput<B>,
    token_wanted: bool,
) where
    B: Clone + PartialEq + Eq + CanonicalSerialize + Debug,
    R: CryptoRandom,
{
    match receive_move_token_output {
        ReceiveMoveTokenOutput::Duplicate => {}
        ReceiveMoveTokenOutput::RetransmitOutgoing(_outgoing_move_token) => {
            // Retransmit last sent token channel message:
            send_commands.set_resend_outgoing(remote_public_key);
            // We should not send any new move token in this case:
            return;
        }
        ReceiveMoveTokenOutput::Received(move_token_received) => {
            send_commands.set_try_send(remote_public_key);

            let MoveTokenReceived {
                // incoming_messages,
                mutations,
                currencies,
                // remote_requests_closed,
                opt_local_relays,
            } = move_token_received;

            // Update address for remote side if necessary:
            if let Some(new_remote_relays) = opt_local_relays {
                let friend = m_state.state().friends.get(remote_public_key).unwrap();
                // Make sure that the newly sent remote address is different than the one we
                // already have:
                if friend.remote_relays != new_remote_relays {
                    // Update remote address:
                    let friend_mutation =
                        FriendMutation::SetRemoteRelays(new_remote_relays.clone());
                    let funder_mutation = FunderMutation::FriendMutation((
                        remote_public_key.clone(),
                        friend_mutation,
                    ));
                    m_state.mutate(funder_mutation);
                }
            }

            // Apply all mutations:
            for tc_mutation in mutations {
                let friend_mutation = FriendMutation::TcMutation(tc_mutation);
                let funder_mutation =
                    FunderMutation::FriendMutation((remote_public_key.clone(), friend_mutation));
                m_state.mutate(funder_mutation);
            }

            // If address update was pending, we can clear it, as this is a proof that the
            // remote side has received our update:
            let friend = m_state.state().friends.get(remote_public_key).unwrap();
            match &friend.sent_local_relays {
                SentLocalRelays::NeverSent | SentLocalRelays::LastSent(_) => {}
                SentLocalRelays::Transition((last_address, _prev_last_address)) => {
                    let c_last_address = last_address.clone();
                    // Update SentLocalRelays:
                    let friend_mutation = FriendMutation::SetSentLocalRelays(
                        SentLocalRelays::LastSent(c_last_address.clone()),
                    );
                    let funder_mutation = FunderMutation::FriendMutation((
                        remote_public_key.clone(),
                        friend_mutation,
                    ));
                    m_state.mutate(funder_mutation);

                    let friend = m_state.state().friends.get(remote_public_key).unwrap();

                    let local_relays = c_last_address
                        .into_iter()
                        .map(RelayAddress::from)
                        .collect::<Vec<_>>();

                    // Notify Channeler to change the friend's address:
                    let update_friend = ChannelerUpdateFriend {
                        friend_public_key: remote_public_key.clone(),
                        friend_relays: friend.remote_relays.clone(),
                        local_relays,
                    };
                    let channeler_config = ChannelerConfig::UpdateFriend(update_friend);
                    outgoing_channeler_config.push(channeler_config);
                }
            }

            for move_token_received_currency in currencies {
                // If remote requests were previously open, and now they were closed:
                if move_token_received_currency.remote_requests_closed {
                    // Cancel all messages pending for this friend with this currency.
                    // We don't want the senders of the requests to wait.
                    cancel_pending_requests(
                        m_state,
                        send_commands,
                        outgoing_control,
                        rng,
                        remote_public_key,
                        &CurrencyChoice::One(move_token_received_currency.currency.clone()),
                    );
                }

                handle_move_token_output(
                    m_state,
                    m_ephemeral,
                    send_commands,
                    outgoing_control,
                    rng,
                    remote_public_key,
                    &move_token_received_currency.currency,
                    move_token_received_currency.incoming_messages,
                );
            }
        }
    }
    if token_wanted {
        send_commands.set_remote_wants_token(&remote_public_key);
    }
}

fn handle_move_token_request<B, R>(
    m_state: &mut MutableFunderState<B>,
    m_ephemeral: &mut MutableEphemeral,
    send_commands: &mut SendCommands,
    outgoing_control: &mut Vec<FunderOutgoingControl<B>>,
    outgoing_channeler_config: &mut Vec<ChannelerConfig<RelayAddress<B>>>,
    rng: &R,
    remote_public_key: &PublicKey,
    friend_move_token_request: MoveTokenRequest<B>,
) -> Result<(), HandleFriendError>
where
    B: Clone + PartialEq + Eq + CanonicalSerialize + Debug,
    R: CryptoRandom,
{
    // Find friend:
    let friend = match m_state.state().friends.get(remote_public_key) {
        Some(friend) => Ok(friend),
        None => Err(HandleFriendError::FriendDoesNotExist),
    }?;

    let token_channel = match &friend.channel_status {
        ChannelStatus::Consistent(channel_consistent) => &channel_consistent.token_channel,
        ChannelStatus::Inconsistent(channel_inconsistent) => {
            let local_reset_terms = channel_inconsistent.local_reset_terms.clone();
            try_reset_channel(
                m_state,
                send_commands,
                remote_public_key,
                &local_reset_terms,
                &friend_move_token_request,
            );
            return Ok(());
        }
    };

    // We will only consider move token messages if we are in a consistent state:
    let receive_move_token_res =
        token_channel.simulate_receive_move_token(friend_move_token_request.move_token);
    let token_wanted = friend_move_token_request.token_wanted;

    match receive_move_token_res {
        Ok(receive_move_token_output) => {
            handle_move_token_success(
                m_state,
                m_ephemeral,
                send_commands,
                outgoing_control,
                outgoing_channeler_config,
                rng,
                remote_public_key,
                receive_move_token_output,
                token_wanted,
            );
        }
        Err(receive_move_token_error) => {
            warn!(
                "simulate_receive_move_token() error: {:?}",
                receive_move_token_error
            );
            handle_move_token_error(
                m_state,
                send_commands,
                outgoing_control,
                rng,
                remote_public_key,
            );
        }
    };
    Ok(())
}

fn handle_inconsistency_error<B, R>(
    m_state: &mut MutableFunderState<B>,
    send_commands: &mut SendCommands,
    outgoing_control: &mut Vec<FunderOutgoingControl<B>>,
    rng: &R,
    remote_public_key: &PublicKey,
    remote_reset_terms: ResetTerms,
) -> Result<(), HandleFriendError>
where
    B: Clone + PartialEq + Eq + CanonicalSerialize + Debug,
    R: CryptoRandom,
{
    // Make sure that friend exists:
    let _ = match m_state.state().friends.get(remote_public_key) {
        Some(friend) => Ok(friend),
        None => Err(HandleFriendError::FriendDoesNotExist),
    }?;

    // Save remote incoming inconsistency details:
    let new_remote_reset_terms = remote_reset_terms;

    // Obtain information about our reset terms:
    let friend = m_state.state().friends.get(remote_public_key).unwrap();
    let (channel_was_consistent, new_local_reset_terms, opt_last_incoming_move_token) =
        match &friend.channel_status {
            ChannelStatus::Consistent(channel_consistent) => {
                let token_channel = &channel_consistent.token_channel;
                if token_channel.get_incoming().is_some() {
                    return Err(HandleFriendError::InconsistencyWhenTokenOwned);
                }
                (
                    true,
                    gen_reset_terms(&token_channel, rng),
                    token_channel.get_last_incoming_move_token_hashed().cloned(),
                )
            }
            ChannelStatus::Inconsistent(channel_inconsistent) => (
                false,
                channel_inconsistent.local_reset_terms.clone(),
                channel_inconsistent.opt_last_incoming_move_token.clone(),
            ),
        };

    if channel_was_consistent {
        // Cancel all pending requests to this friend:
        cancel_pending_requests(
            m_state,
            send_commands,
            outgoing_control,
            rng,
            remote_public_key,
            &CurrencyChoice::All,
        );
    }

    // Keep outgoing InconsistencyError message details in memory:
    let channel_inconsistent = ChannelInconsistent {
        opt_last_incoming_move_token,
        local_reset_terms: new_local_reset_terms.clone(),
        opt_remote_reset_terms: Some(new_remote_reset_terms),
    };
    let friend_mutation = FriendMutation::SetInconsistent(channel_inconsistent);
    let funder_mutation =
        FunderMutation::FriendMutation((remote_public_key.clone(), friend_mutation));
    m_state.mutate(funder_mutation);

    // Send an outgoing inconsistency message if required:
    if channel_was_consistent {
        send_commands.set_try_send(remote_public_key);
    }
    Ok(())
}

pub fn handle_friend_message<B, R>(
    m_state: &mut MutableFunderState<B>,
    m_ephemeral: &mut MutableEphemeral,
    send_commands: &mut SendCommands,
    outgoing_control: &mut Vec<FunderOutgoingControl<B>>,
    outgoing_channeler_config: &mut Vec<ChannelerConfig<RelayAddress<B>>>,
    rng: &R,
    remote_public_key: &PublicKey,
    friend_message: FriendMessage<B>,
) -> Result<(), HandleFriendError>
where
    B: Clone + PartialEq + Eq + CanonicalSerialize + Debug,
    R: CryptoRandom,
{
    // Make sure that friend exists:
    let _ = match m_state.state().friends.get(remote_public_key) {
        Some(friend) => Ok(friend),
        None => Err(HandleFriendError::FriendDoesNotExist),
    }?;

    match friend_message {
        FriendMessage::MoveTokenRequest(friend_move_token_request) => handle_move_token_request(
            m_state,
            m_ephemeral,
            send_commands,
            outgoing_control,
            outgoing_channeler_config,
            rng,
            remote_public_key,
            friend_move_token_request,
        ),

        FriendMessage::InconsistencyError(remote_reset_terms) => handle_inconsistency_error(
            m_state,
            send_commands,
            outgoing_control,
            rng,
            remote_public_key,
            remote_reset_terms,
        ),
    }
}
