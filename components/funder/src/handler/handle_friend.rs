use std::fmt::Debug;
use crypto::crypto_rand::CryptoRandom;
use crypto::identity::{PublicKey, Signature, SIGNATURE_LEN};

use common::canonical_serialize::CanonicalSerialize;

use proto::funder::messages::{RequestSendFunds, ResponseSendFunds,
    FailureSendFunds, FriendMessage,
    MoveTokenRequest, ResetTerms, PendingRequest, ResponseReceived,
    FunderOutgoingControl, ResponseSendFundsResult};
use proto::funder::signature_buff::{prepare_receipt, verify_move_token};

use crate::mutual_credit::incoming::{IncomingResponseSendFunds, 
    IncomingFailureSendFunds, IncomingMessage};
use crate::token_channel::{ReceiveMoveTokenOutput, 
    MoveTokenReceived, TokenChannel};

use crate::types::{create_pending_request, ChannelerConfig,
                    ChannelerUpdateFriend};

use crate::state::FunderMutation;
use crate::friend::{FriendMutation, 
    ResponseOp, ChannelStatus, ChannelInconsistent,
    SentLocalAddress};

use crate::ephemeral::{Ephemeral, EphemeralMutation};
use crate::freeze_guard::FreezeGuardMutation;

use crate::handler::handler::{MutableFunderState, MutableEphemeral, 
    is_friend_ready, find_request_origin, add_local_freezing_link};
use crate::handler::sender::SendCommands;
use crate::handler::canceler::{cancel_local_pending_requests, 
    cancel_pending_user_requests, cancel_pending_requests,
    reply_with_failure};


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


pub fn gen_reset_terms<A,R>(token_channel: &TokenChannel<A>, 
                             rng: &R) -> ResetTerms 
where
    A: CanonicalSerialize + Clone,
    R: CryptoRandom,
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
pub fn try_reset_channel<A>(m_state: &mut MutableFunderState<A>,
                            send_commands: &mut SendCommands,
                            friend_public_key: &PublicKey,
                            local_reset_terms: &ResetTerms,
                            move_token_request: &MoveTokenRequest<A>) 
where
    A: CanonicalSerialize + Clone + Debug,
{
    let move_token = &move_token_request.friend_move_token;

    // Check if incoming message is a valid attempt to reset the channel:
    if move_token.old_token != local_reset_terms.reset_token 
        || !move_token.operations.is_empty()
        || move_token.opt_local_address.is_some()
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
        local_reset_terms.balance_for_reset);

    // This is a reset message. We reset the token channel:
    let friend_mutation = FriendMutation::SetConsistent(token_channel);
    let funder_mutation = FunderMutation::FriendMutation((friend_public_key.clone(), friend_mutation));
    m_state.mutate(funder_mutation);

    send_commands.set_try_send(friend_public_key);
    if move_token_request.token_wanted {
        send_commands.set_remote_wants_token(friend_public_key);
    }
}

/// Forward a request message to the relevant friend and token channel.
fn forward_request<A>(m_state: &mut MutableFunderState<A>,
                      send_commands: &mut SendCommands,
                      request_send_funds: RequestSendFunds) 
where
    A: CanonicalSerialize + Clone,
{
    let index = request_send_funds.route.pk_to_index(&m_state.state().local_public_key)
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

/*
/// Create a (signed) failure message for a given request_id.
/// We are the reporting_public_key for this failure message.
fn create_response_message<A,R>(state: &FunderState<A>, 
                              rng: &R,
                              request_send_funds: RequestSendFunds) 
    -> UnsignedResponseSendFunds 

where
    A: Clone,
    R: CryptoRandom,
{

    let rand_nonce = RandValue::new(rng);
    let local_public_key = state.local_public_key.clone();

    let mut u_response_send_funds = ResponseSendFunds {
        request_id: request_send_funds.request_id,
        rand_nonce,
        signature: (),
    };

    u_response_send_funds
}
*/

fn handle_request_send_funds<A>(m_state: &mut MutableFunderState<A>,
                                ephemeral: &Ephemeral,
                                send_commands: &mut SendCommands,
                                remote_public_key: &PublicKey,
                                mut request_send_funds: RequestSendFunds) 
where
    A: CanonicalSerialize + Clone,
{
    // Find ourselves on the route. If we are not there, abort.
    let remote_index = request_send_funds.route.find_pk_pair(
        &remote_public_key, 
        &m_state.state().local_public_key).unwrap();

    let local_index = remote_index.checked_add(1).unwrap();
    let next_index = local_index.checked_add(1).unwrap();
    if next_index >= request_send_funds.route.len() {
        // We are the destination of this request. We return a response:
        let pending_request = create_pending_request(&request_send_funds);
        let u_response_op = ResponseOp::UnsignedResponse(pending_request);
        let friend_mutation = FriendMutation::PushBackPendingResponse(u_response_op);
        let funder_mutation = FunderMutation::FriendMutation((remote_public_key.clone(), friend_mutation));
        m_state.mutate(funder_mutation);
        send_commands.set_try_send(&remote_public_key);
        return;
    }


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

    if !friend_ready {
        reply_with_failure(m_state, 
                           send_commands,
                           remote_public_key, 
                           &request_send_funds);
        return;
    } 
    // Add our freezing link:
    add_local_freezing_link(m_state.state(), &mut request_send_funds);

    // Queue message to the next node.
    forward_request(m_state, 
                    send_commands, 
                    request_send_funds);
}


fn handle_response_send_funds<A>(m_state: &mut MutableFunderState<A>,
                                 send_commands: &mut SendCommands,
                                 outgoing_control: &mut Vec<FunderOutgoingControl<A>>,
                                 response_send_funds: ResponseSendFunds,
                                 pending_request: PendingRequest) 
where
    A: CanonicalSerialize + Clone,
{

    match find_request_origin(m_state.state(), 
                              &response_send_funds.request_id).cloned() {
        None => {
            // We are the origin of this request, and we got a response.
            // We provide a receipt to the user:
            let receipt = prepare_receipt(&response_send_funds,
                                          &pending_request);

            let response_send_funds_result = ResponseSendFundsResult::Success(receipt.clone());
            outgoing_control.push(FunderOutgoingControl::ResponseReceived(
                ResponseReceived {
                    request_id: pending_request.request_id.clone(),
                    result: response_send_funds_result,
                }
            ));
            // We make our own copy of the receipt, in case the user abruptly crashes.
            // In that case the user will be able to obtain the receipt again later.
            let funder_mutation = FunderMutation::AddReceipt((pending_request.request_id, receipt));
            m_state.mutate(funder_mutation);
        },
        Some(friend_public_key) => {
            // Queue this response message to another token channel:
            let response_op = ResponseOp::Response(response_send_funds);
            let friend_mutation = FriendMutation::PushBackPendingResponse(response_op);
            let funder_mutation = FunderMutation::FriendMutation((friend_public_key.clone(), friend_mutation));
            m_state.mutate(funder_mutation);

            send_commands.set_try_send(&friend_public_key);
        },
    }
}

fn handle_failure_send_funds<A>(m_state: &mut MutableFunderState<A>,
                                send_commands: &mut SendCommands,
                                outgoing_control: &mut Vec<FunderOutgoingControl<A>>,
                                failure_send_funds: FailureSendFunds,
                                pending_request: PendingRequest) 
where
    A: CanonicalSerialize + Clone,
{

    match find_request_origin(m_state.state(), 
                              &failure_send_funds.request_id).cloned() {
        None => {
            // We are the origin of this request, and we got a failure
            // We should pass it back to crypter.


            let response_send_funds_result = ResponseSendFundsResult::Failure(failure_send_funds.reporting_public_key);
            outgoing_control.push(FunderOutgoingControl::ResponseReceived(
                ResponseReceived {
                    request_id: pending_request.request_id,
                    result: response_send_funds_result,
                }
            ));
        },
        Some(friend_public_key) => {
            // Queue this failure message to another token channel:
            let failure_op = ResponseOp::Failure(failure_send_funds);
            let friend_mutation = FriendMutation::PushBackPendingResponse(failure_op);
            let funder_mutation = FunderMutation::FriendMutation((friend_public_key.clone(), friend_mutation));
            m_state.mutate(funder_mutation);

            send_commands.set_try_send(&friend_public_key);
        },
    };
}

/// Process valid incoming operations from remote side.
fn handle_move_token_output<A>(m_state: &mut MutableFunderState<A>,
                               m_ephemeral: &mut MutableEphemeral,
                               send_commands: &mut SendCommands,
                               outgoing_control: &mut Vec<FunderOutgoingControl<A>>,
                               remote_public_key: &PublicKey,
                               incoming_messages: Vec<IncomingMessage>) 
where
    A: CanonicalSerialize + Clone,
{

    for incoming_message in incoming_messages {
        match incoming_message {
            IncomingMessage::Request(request_send_funds) => {
                handle_request_send_funds(m_state, 
                                          m_ephemeral.ephemeral(),
                                          send_commands,
                                          remote_public_key,
                                          request_send_funds);
            },
            IncomingMessage::Response(IncomingResponseSendFunds {
                                            pending_request, incoming_response}) => {

                let freeze_guard_mutation = FreezeGuardMutation::SubFrozenCredit(
                    (pending_request.route.clone(), pending_request.dest_payment));
                let ephemeral_mutation = EphemeralMutation::FreezeGuardMutation(freeze_guard_mutation);
                m_ephemeral.mutate(ephemeral_mutation);
                handle_response_send_funds(m_state, 
                                           send_commands,
                                           outgoing_control,
                                           incoming_response, 
                                           pending_request);
            },
            IncomingMessage::Failure(IncomingFailureSendFunds {
                                            pending_request, incoming_failure}) => {

                let freeze_guard_mutation = FreezeGuardMutation::SubFrozenCredit(
                    (pending_request.route.clone(), pending_request.dest_payment));
                let ephemeral_mutation = EphemeralMutation::FreezeGuardMutation(freeze_guard_mutation);
                m_ephemeral.mutate(ephemeral_mutation);
                handle_failure_send_funds(m_state, 
                                          send_commands,
                                          outgoing_control,
                                          incoming_failure, 
                                          pending_request);
            },
        }
    }
}


/// Handle an error with incoming move token.
fn handle_move_token_error<A,R>(m_state: &mut MutableFunderState<A>,
                                m_ephemeral: &mut MutableEphemeral,
                                send_commands: &mut SendCommands,
                                outgoing_control: &mut Vec<FunderOutgoingControl<A>>,
                                rng: &R,
                                remote_public_key: &PublicKey)
where
    A: CanonicalSerialize + Clone,
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
    cancel_local_pending_requests(
        m_state,
        m_ephemeral,
        send_commands,
        outgoing_control,
        remote_public_key);
    // Cancel all pending requests to this friend:
    cancel_pending_requests(
            m_state,
            send_commands,
            outgoing_control,
            remote_public_key);
    cancel_pending_user_requests(
            m_state,
            outgoing_control,
            remote_public_key);

    // Keep outgoing InconsistencyError message details in memory:
    let channel_inconsistent = ChannelInconsistent {
        opt_last_incoming_move_token,
        local_reset_terms,
        opt_remote_reset_terms: None,
    };
    let friend_mutation = FriendMutation::SetInconsistent(channel_inconsistent);
    let funder_mutation = FunderMutation::FriendMutation((remote_public_key.clone(), friend_mutation));
    m_state.mutate(funder_mutation);
    send_commands.set_try_send(remote_public_key);
}


/// Handle success with incoming move token.
fn handle_move_token_success<A>(m_state: &mut MutableFunderState<A>,
                                m_ephemeral: &mut MutableEphemeral,
                                send_commands: &mut SendCommands,
                                outgoing_control: &mut Vec<FunderOutgoingControl<A>>,
                                outgoing_channeler_config: &mut Vec<ChannelerConfig<A>>,
                                remote_public_key: &PublicKey,
                                receive_move_token_output: ReceiveMoveTokenOutput<A>,
                                token_wanted: bool) 
where
    A: CanonicalSerialize + Clone + Eq,
{
    match receive_move_token_output {
        ReceiveMoveTokenOutput::Duplicate => {},
        ReceiveMoveTokenOutput::RetransmitOutgoing(_outgoing_move_token) => {
            // Retransmit last sent token channel message:
            send_commands.set_resend_outgoing(remote_public_key);
            // We should not send any new move token in this case:
            return;
        },
        ReceiveMoveTokenOutput::Received(move_token_received) => {
            send_commands.set_try_send(remote_public_key);

            let MoveTokenReceived {
                incoming_messages, 
                mutations, 
                remote_requests_closed, 
                opt_local_address
            } = move_token_received;

            // Update address for remote side if necessary:
            if let Some(new_remote_address) = opt_local_address {
                let friend = m_state.state().friends.get(remote_public_key).unwrap();
                // Make sure that the newly sent remote address is different than the one we
                // already have:
                if friend.remote_address != new_remote_address {
                    // Update remote address:
                    let friend_mutation = FriendMutation::SetRemoteAddress(new_remote_address);
                    let funder_mutation = FunderMutation::FriendMutation((remote_public_key.clone(), friend_mutation));
                    m_state.mutate(funder_mutation);
                }
            }

            // Apply all mutations:
            for tc_mutation in mutations {
                let friend_mutation = FriendMutation::TcMutation(tc_mutation);
                let funder_mutation = FunderMutation::FriendMutation((remote_public_key.clone(), friend_mutation));
                m_state.mutate(funder_mutation);
            }

            // If address update was pending, we can clear it, as this is a proof that the
            // remote side has received our update:
            let friend = m_state.state().friends.get(remote_public_key).unwrap();
            match &friend.sent_local_address {
                SentLocalAddress::NeverSent |
                SentLocalAddress::LastSent(_) => {},
                SentLocalAddress::Transition((last_address, _prev_last_address)) => {
                    let c_last_address = last_address.clone();
                    // Update SentLocalAddress:
                    let friend_mutation = FriendMutation::SetSentLocalAddress(SentLocalAddress::LastSent(c_last_address.clone()));
                    let funder_mutation = FunderMutation::FriendMutation((remote_public_key.clone(), friend_mutation));
                    m_state.mutate(funder_mutation);

                    let friend = m_state.state().friends.get(remote_public_key).unwrap();

                    // Notify Channeler to change the friend's address:
                    let update_friend = ChannelerUpdateFriend {
                        friend_public_key: remote_public_key.clone(),
                        friend_address: friend.remote_address.clone(),
                        local_addresses: vec![c_last_address.clone()],
                    };
                    let channeler_config = ChannelerConfig::UpdateFriend(update_friend);
                    outgoing_channeler_config.push(channeler_config);
                },
            }

            // If remote requests were previously open, and now they were closed:
            if remote_requests_closed {
                // Cancel all messages pending for this friend. 
                // We don't want the senders of the requests to wait.
                cancel_pending_requests(
                    m_state,
                    send_commands,
                    outgoing_control,
                    remote_public_key);
                cancel_pending_user_requests(
                    m_state,
                    outgoing_control,
                    remote_public_key);
            }

            handle_move_token_output(m_state, 
                                     m_ephemeral,
                                     send_commands,
                                     outgoing_control,
                                     remote_public_key,
                                     incoming_messages);

        },
    }
    if token_wanted {
        send_commands.set_remote_wants_token(&remote_public_key);
    }
}


fn handle_move_token_request<A,R>(m_state: &mut MutableFunderState<A>, 
                                m_ephemeral: &mut MutableEphemeral,
                                send_commands: &mut SendCommands,
                                outgoing_control: &mut Vec<FunderOutgoingControl<A>>,
                                outgoing_channeler_config: &mut Vec<ChannelerConfig<A>>,
                                rng: &R,
                                remote_public_key: &PublicKey,
                                friend_move_token_request: MoveTokenRequest<A>) 
    -> Result<(), HandleFriendError> 
where
    A: CanonicalSerialize + Clone + Eq + Debug,
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
            try_reset_channel(m_state, 
                              send_commands,
                              remote_public_key, 
                              &channel_inconsistent.local_reset_terms.clone(),
                              &friend_move_token_request);
            return Ok(());
        }
    };

    // We will only consider move token messages if we are in a consistent state:
    let receive_move_token_res = token_channel.simulate_receive_move_token(
        friend_move_token_request.friend_move_token);
    let token_wanted = friend_move_token_request.token_wanted;

    match receive_move_token_res {
        Ok(receive_move_token_output) => {
            handle_move_token_success(m_state, 
                                      m_ephemeral,
                                      send_commands,
                                      outgoing_control,
                                      outgoing_channeler_config,
                                      remote_public_key,
                                      receive_move_token_output,
                                      token_wanted);
        },
        Err(_receive_move_token_error) => {
            handle_move_token_error(m_state, 
                                    m_ephemeral,
                                    send_commands,
                                    outgoing_control,
                                    rng,
                                    remote_public_key);
        },
    };
    Ok(())
}

fn handle_inconsistency_error<A,R>(m_state: &mut MutableFunderState<A>,
                                   send_commands: &mut SendCommands,
                                   outgoing_control: &mut Vec<FunderOutgoingControl<A>>,
                                   rng: &R,
                                   remote_public_key: &PublicKey,
                                   remote_reset_terms: ResetTerms)
                                    -> Result<(), HandleFriendError> 
where
    A: CanonicalSerialize + Clone,
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
        remote_public_key);
    cancel_pending_user_requests(
        m_state,
        outgoing_control,
        remote_public_key);

    // Save remote incoming inconsistency details:
    let new_remote_reset_terms = remote_reset_terms;

    // Obtain information about our reset terms:
    let friend = m_state.state().friends.get(remote_public_key).unwrap();
    let (should_send_outgoing, 
         new_local_reset_terms, 
         opt_last_incoming_move_token) = match &friend.channel_status {
        ChannelStatus::Consistent(token_channel) => {
            if !token_channel.is_outgoing() {
                return Err(HandleFriendError::InconsistencyWhenTokenOwned);
            }
            (true, 
             gen_reset_terms(&token_channel, rng),
             token_channel.get_last_incoming_move_token_hashed().cloned())
        },
        ChannelStatus::Inconsistent(channel_inconsistent) => 
            (false, 
             channel_inconsistent.local_reset_terms.clone(),
             channel_inconsistent.opt_last_incoming_move_token.clone()),
    };

    // Keep outgoing InconsistencyError message details in memory:
    let channel_inconsistent = ChannelInconsistent {
        opt_last_incoming_move_token,
        local_reset_terms: new_local_reset_terms.clone(),
        opt_remote_reset_terms: Some(new_remote_reset_terms),
    };
    let friend_mutation = FriendMutation::SetInconsistent(channel_inconsistent);
    let funder_mutation = FunderMutation::FriendMutation((remote_public_key.clone(), friend_mutation));
    m_state.mutate(funder_mutation);

    // Send an outgoing inconsistency message if required:
    if should_send_outgoing {
        send_commands.set_try_send(remote_public_key);
    }
    Ok(())
}

pub fn handle_friend_message<A,R>(m_state: &mut MutableFunderState<A>, 
                                  m_ephemeral: &mut MutableEphemeral,
                                  send_commands: &mut SendCommands,
                                  outgoing_control: &mut Vec<FunderOutgoingControl<A>>,
                                  outgoing_channeler_config: &mut Vec<ChannelerConfig<A>>,
                                  rng: &R,
                                  remote_public_key: &PublicKey, 
                                  friend_message: FriendMessage<A>)
                                    -> Result<(), HandleFriendError> 
where
    A: CanonicalSerialize + Clone + Eq + Debug,
    R: CryptoRandom,
{
    // Make sure that friend exists:
    let _ = match m_state.state().friends.get(remote_public_key) {
        Some(friend) => Ok(friend),
        None => Err(HandleFriendError::FriendDoesNotExist),
    }?;

    match friend_message {
        FriendMessage::MoveTokenRequest(friend_move_token_request) =>
            handle_move_token_request(m_state,
                                      m_ephemeral,
                                      send_commands,
                                      outgoing_control,
                                      outgoing_channeler_config,
                                      rng,
                                      remote_public_key, 
                                      friend_move_token_request),

        FriendMessage::InconsistencyError(remote_reset_terms) =>
            handle_inconsistency_error(m_state, 
                                       send_commands,
                                       outgoing_control,
                                       rng,
                                       remote_public_key, 
                                       remote_reset_terms),
    }
}
