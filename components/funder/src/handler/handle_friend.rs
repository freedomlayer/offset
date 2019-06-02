use common::canonical_serialize::CanonicalSerialize;
use std::fmt::Debug;

use crypto::crypto_rand::CryptoRandom;
use crypto::identity::{PublicKey, Signature, SIGNATURE_LEN};
use crypto::uid::Uid;

use proto::app_server::messages::RelayAddress;
use proto::funder::messages::{
    CancelSendFundsOp, ChannelerUpdateFriend, CollectSendFundsOp, FriendMessage,
    FunderOutgoingControl, MoveTokenRequest, PendingTransaction, RequestResult, RequestSendFundsOp,
    ResetTerms, ResponseSendFundsOp, TransactionResult,
};
use proto::funder::signature_buff::{prepare_commit, prepare_receipt, verify_move_token};

use crate::mutual_credit::incoming::{
    IncomingCancelSendFundsOp, IncomingCollectSendFundsOp, IncomingMessage,
    IncomingResponseSendFundsOp,
};
use crate::token_channel::{MoveTokenReceived, ReceiveMoveTokenOutput, TokenChannel};

use crate::types::{create_pending_transaction, ChannelerConfig};

use crate::friend::{
    BackwardsOp, ChannelInconsistent, ChannelStatus, FriendMutation, SentLocalRelays,
};
use crate::state::{FunderMutation, Payment};

use crate::ephemeral::Ephemeral;

use crate::handler::canceler::{
    cancel_local_pending_transactions, cancel_pending_requests, cancel_pending_user_requests,
    remove_transaction, reply_with_cancel,
};
use crate::handler::sender::SendCommands;
use crate::handler::state_wrap::{MutableEphemeral, MutableFunderState};
use crate::handler::utils::{find_request_origin, is_friend_ready};

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
    let mut buff = [0; SIGNATURE_LEN];
    rng.fill(&mut buff).unwrap();
    Signature::from(buff)
}

pub fn gen_reset_terms<B, R>(token_channel: &TokenChannel<B>, rng: &R) -> ResetTerms
where
    R: CryptoRandom,
    B: Clone + PartialEq + Eq + CanonicalSerialize + Debug,
{
    // We add 2 for the new counter in case
    // the remote side has already used the next counter.
    let reset_token = gen_channel_reset_token(rng);

    ResetTerms {
        reset_token,
        // TODO: Should we do something other than wrapping_add(1)?
        // 2**64 inconsistencies are required for an overflow.
        inconsistency_counter: token_channel.get_inconsistency_counter().wrapping_add(1),
        balance_for_reset: token_channel.get_mutual_credit().balance_for_reset(),
    }
}

/// Check if channel reset is required (Remove side used the RESET token)
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
    let move_token = &move_token_request.friend_move_token;

    // Check if incoming message is a valid attempt to reset the channel:
    if move_token.old_token != local_reset_terms.reset_token
        || !move_token.operations.is_empty()
        || move_token.opt_local_relays.is_some()
        || move_token.inconsistency_counter != local_reset_terms.inconsistency_counter
        || move_token.move_token_counter != 0
        || move_token.balance != local_reset_terms.balance_for_reset.checked_neg().unwrap()
        || move_token.local_pending_debt != 0
        || move_token.remote_pending_debt != 0
        || !verify_move_token(move_token, friend_public_key)
    {
        send_commands.set_resend_outgoing(friend_public_key);
        return;
    }

    let token_channel = TokenChannel::new_from_remote_reset(
        &m_state.state().local_public_key,
        friend_public_key,
        move_token,
        local_reset_terms.balance_for_reset,
    );

    // This is a reset message. We reset the token channel:
    let friend_mutation = FriendMutation::SetConsistent(token_channel);
    let funder_mutation =
        FunderMutation::FriendMutation((friend_public_key.clone(), friend_mutation));
    m_state.mutate(funder_mutation);

    send_commands.set_try_send(friend_public_key);
    if move_token_request.token_wanted {
        send_commands.set_remote_wants_token(friend_public_key);
    }
}

/// Forward a request message to the relevant friend and token channel.
fn forward_request<B>(
    m_state: &mut MutableFunderState<B>,
    send_commands: &mut SendCommands,
    request_send_funds: RequestSendFundsOp,
) where
    B: Clone + PartialEq + Eq + CanonicalSerialize + Debug,
{
    let index = request_send_funds
        .route
        .pk_to_index(&m_state.state().local_public_key)
        .unwrap();
    let next_index = index.checked_add(1).unwrap();
    let next_pk = request_send_funds.route.index_to_pk(next_index).unwrap();

    // Queue message to the relevant friend. Later this message will be queued to a specific
    // available token channel:
    let friend_mutation = FriendMutation::PushBackPendingRequest(request_send_funds.clone());
    let funder_mutation = FunderMutation::FriendMutation((next_pk.clone(), friend_mutation));
    m_state.mutate(funder_mutation);
    send_commands.set_try_send(&next_pk);
}

fn handle_request_send_funds<B>(
    m_state: &mut MutableFunderState<B>,
    ephemeral: &Ephemeral,
    send_commands: &mut SendCommands,
    remote_public_key: &PublicKey,
    mut request_send_funds: RequestSendFundsOp,
) where
    B: Clone + PartialEq + Eq + CanonicalSerialize + Debug,
{
    // Find ourselves on the route. If we are not there, abort.
    let remote_index = request_send_funds
        .route
        .find_pk_pair(&remote_public_key, &m_state.state().local_public_key)
        .unwrap();

    let local_index = remote_index.checked_add(1).unwrap();
    let next_index = local_index.checked_add(1).unwrap();
    if next_index >= request_send_funds.route.len() {
        // We are the destination of this request.

        // First make sure that we have a matching open invoice for this transaction:
        let is_invoice_match = if let Some(open_invoice) = m_state
            .state()
            .open_invoices
            .get(&request_send_funds.invoice_id)
        {
            open_invoice.total_dest_payment == request_send_funds.total_dest_payment
                && request_send_funds.dest_payment <= request_send_funds.total_dest_payment
        } else {
            false
        };

        if !is_invoice_match {
            reply_with_cancel(
                m_state,
                send_commands,
                remote_public_key,
                &request_send_funds.request_id,
            );
            return;
        };

        // We return a response:
        let pending_transaction = create_pending_transaction(&request_send_funds);
        m_state.queue_unsigned_response(remote_public_key.clone(), pending_transaction);
        /*
        let u_response_op = BackwardsOp::UnsignedResponse(pending_transaction);
        let friend_mutation = FriendMutation::PushBackPendingResponse(u_response_op);
        let funder_mutation =
           FunderMutation::FriendMutation((remote_public_key.clone(), friend_mutation));
        m_state.mutate(funder_mutation);
        */
        send_commands.set_try_send(&remote_public_key);
        return;
    }

    // We are not the destination of this request.
    // The node on the route has to be one of our friends:
    let next_public_key = request_send_funds.route.index_to_pk(next_index).unwrap();
    let friend_exists = m_state.state().friends.contains_key(next_public_key);

    // This friend must be considered online for us to forward the message.
    // If we forward the request to an offline friend, the request could be stuck for a long
    // time before a response arrives.
    let friend_ready = if friend_exists {
        is_friend_ready(m_state.state(), ephemeral, &next_public_key)
    } else {
        false
    };

    // Attempt to take our fee for forwarding the request.
    // Note that the rate is determined by the rate we set with the node that sent us the request
    // (And **not** with the node that we forward the request to).
    let rate = &m_state.state().friends.get(remote_public_key).unwrap().rate;
    let opt_local_fee = rate.calc_fee(request_send_funds.dest_payment);

    let request_id = request_send_funds.request_id;

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

    let request_send_funds = match (opt_request_send_funds, friend_ready) {
        (Some(request_send_funds), true) => request_send_funds,
        _ => {
            reply_with_cancel(m_state, send_commands, remote_public_key, &request_id);
            return;
        }
    };

    // Queue message to the next node.
    forward_request(m_state, send_commands, request_send_funds);
}

fn handle_response_send_funds<B>(
    m_state: &mut MutableFunderState<B>,
    send_commands: &mut SendCommands,
    outgoing_control: &mut Vec<FunderOutgoingControl<B>>,
    response_send_funds: ResponseSendFundsOp,
    pending_transaction: PendingTransaction,
) where
    B: Clone + PartialEq + Eq + CanonicalSerialize + Debug,
{
    match find_request_origin(m_state.state(), &response_send_funds.request_id).cloned() {
        None => {
            // Keep the response:
            let funder_mutation =
                FunderMutation::SetTransactionResponse(response_send_funds.clone());
            m_state.mutate(funder_mutation);

            // Send transaction result to user:
            let src_plain_lock = m_state
                .state()
                .open_transactions
                .get(&response_send_funds.request_id)
                .unwrap()
                .src_plain_lock
                .clone();

            let commit = prepare_commit(&response_send_funds, &pending_transaction, src_plain_lock);

            let transaction_result = TransactionResult {
                request_id: response_send_funds.request_id,
                result: RequestResult::Success(commit),
            };
            outgoing_control.push(FunderOutgoingControl::TransactionResult(transaction_result));
        }
        Some(friend_public_key) => {
            // Queue this response message to another token channel:
            let response_op = BackwardsOp::Response(response_send_funds);
            let friend_mutation = FriendMutation::PushBackPendingBackwardsOp(response_op);
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
    cancel_send_funds: CancelSendFundsOp,
    pending_transaction: PendingTransaction,
) where
    B: Clone + PartialEq + Eq + CanonicalSerialize + Debug,
    R: CryptoRandom,
{
    match find_request_origin(m_state.state(), &cancel_send_funds.request_id).cloned() {
        None => {
            // We are the origin of this request, and we got a cancellation.

            // Update buyer transactions (requests that were originated by us):
            remove_transaction(m_state, rng, &cancel_send_funds.request_id);

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
            let friend_mutation = FriendMutation::PushBackPendingBackwardsOp(cancel_op);
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
    rng: &R,
    collect_send_funds: CollectSendFundsOp,
    pending_transaction: PendingTransaction,
) where
    B: Clone + PartialEq + Eq + CanonicalSerialize + Debug,
    R: CryptoRandom,
{
    // Check if we are the origin of this transaction (Did we send the RequestSendFundsOp
    // message?):
    match find_request_origin(m_state.state(), &collect_send_funds.request_id).cloned() {
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
            let opt_new_payment = match payment {
                Payment::NewTransactions(new_transactions) => {
                    // Create a Receipt:
                    let receipt = prepare_receipt(
                        &collect_send_funds,
                        open_transaction.opt_response.as_ref().unwrap(),
                        &pending_transaction,
                    );
                    let ack_uid = Uid::new(rng);
                    Some(Payment::Success((
                        new_transactions.num_transactions.checked_sub(1).unwrap(),
                        receipt,
                        ack_uid,
                    )))
                }
                Payment::InProgress(num_transactions) => {
                    // Create a Receipt:
                    let receipt = prepare_receipt(
                        &collect_send_funds,
                        open_transaction.opt_response.as_ref().unwrap(),
                        &pending_transaction,
                    );
                    let ack_uid = Uid::new(rng);
                    Some(Payment::Success((
                        num_transactions.checked_sub(1).unwrap(),
                        receipt,
                        ack_uid,
                    )))
                }
                Payment::Success((num_transactions, receipt, ack_uid)) => Some(Payment::Success((
                    num_transactions.checked_sub(1).unwrap(),
                    receipt.clone(),
                    *ack_uid,
                ))),
                Payment::Canceled(_) => unreachable!(),
                Payment::AfterSuccessAck(num_transactions) => {
                    let new_num_transactions = num_transactions.checked_sub(1).unwrap();
                    if new_num_transactions > 0 {
                        Some(Payment::AfterSuccessAck(new_num_transactions))
                    } else {
                        None
                    }
                }
            };

            let funder_mutation = if let Some(new_payment) = opt_new_payment {
                // Update payment:
                FunderMutation::UpdatePayment((open_transaction.payment_id, new_payment))
            } else {
                FunderMutation::RemovePayment(open_transaction.payment_id)
                // Remove payment:
            };
            m_state.mutate(funder_mutation);

            // Remove transaction:
            let funder_mutation = FunderMutation::RemoveTransaction(collect_send_funds.request_id);
            m_state.mutate(funder_mutation);
        }
        Some(friend_public_key) => {
            // Queue this Collect message to another token channel:
            let collect_op = BackwardsOp::Collect(collect_send_funds);
            let friend_mutation = FriendMutation::PushBackPendingBackwardsOp(collect_op);
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
                    rng,
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
        ChannelStatus::Consistent(token_channel) => token_channel,
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
    );
    cancel_pending_user_requests(m_state, outgoing_control, rng, remote_public_key);

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
                incoming_messages,
                mutations,
                remote_requests_closed,
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

            // If remote requests were previously open, and now they were closed:
            if remote_requests_closed {
                // Cancel all messages pending for this friend.
                // We don't want the senders of the requests to wait.
                cancel_pending_requests(
                    m_state,
                    send_commands,
                    outgoing_control,
                    rng,
                    remote_public_key,
                );
                cancel_pending_user_requests(m_state, outgoing_control, rng, remote_public_key);
            }

            handle_move_token_output(
                m_state,
                m_ephemeral,
                send_commands,
                outgoing_control,
                rng,
                remote_public_key,
                incoming_messages,
            );
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
        ChannelStatus::Consistent(token_channel) => token_channel,
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
        token_channel.simulate_receive_move_token(friend_move_token_request.friend_move_token);
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
        Err(_receive_move_token_error) => {
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

    // Cancel all pending requests to this friend:
    cancel_pending_requests(
        m_state,
        send_commands,
        outgoing_control,
        rng,
        remote_public_key,
    );
    cancel_pending_user_requests(m_state, outgoing_control, rng, remote_public_key);

    // Save remote incoming inconsistency details:
    let new_remote_reset_terms = remote_reset_terms;

    // Obtain information about our reset terms:
    let friend = m_state.state().friends.get(remote_public_key).unwrap();
    let (should_send_outgoing, new_local_reset_terms, opt_last_incoming_move_token) =
        match &friend.channel_status {
            ChannelStatus::Consistent(token_channel) => {
                if !token_channel.is_outgoing() {
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
    if should_send_outgoing {
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
