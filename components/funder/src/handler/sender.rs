use std::collections::HashMap;

use crypto::identity::PublicKey;
use crypto::crypto_rand::{RandValue, CryptoRandom};

use proto::funder::messages::{FriendTcOp, FriendMessage, RequestsStatus, MoveTokenRequest};
use common::canonical_serialize::CanonicalSerialize;

use identity::IdentityClient;

use crate::types::{create_pending_request, sign_move_token,
                    create_response_send_funds, create_failure_send_funds,
                    create_unsigned_move_token};
use crate::mutual_credit::outgoing::{QueueOperationError, OutgoingMc};

use crate::friend::{FriendMutation, ResponseOp, 
    ChannelStatus, SentLocalAddress, ChannelInconsistent};
use crate::token_channel::{TokenChannel, TcMutation, TcDirection, SetDirection};

use crate::freeze_guard::FreezeGuardMutation;

use crate::state::FunderMutation;
use crate::ephemeral::EphemeralMutation;

use crate::handler::handler::{MutableFunderState, MutableEphemeral};


#[derive(Debug, Clone)]
pub struct FriendSendCommands {
    /// Try to send whatever possible through this friend.
    pub try_send: bool,
    /// Resend the outgoing move token message
    pub resend_outgoing: bool,
    /// Remote friend wants the token.
    pub remote_wants_token: bool,
    /// We want to perform a local reset
    pub local_reset: bool,
}

impl FriendSendCommands {
    fn new() -> Self {
        FriendSendCommands {
            try_send: false,
            resend_outgoing: false,
            remote_wants_token: false,
            local_reset: false,
        }
    }
}

pub type OutgoingMessage<A> = (PublicKey, FriendMessage<A>);



#[derive(Clone)]
pub struct SendCommands {
    pub send_commands: HashMap<PublicKey, FriendSendCommands>
}

impl SendCommands {
    pub fn new() -> Self {
        SendCommands {
            send_commands: HashMap::new(),
        }
    }

    pub fn set_try_send(&mut self, friend_public_key: &PublicKey) {
        let friend_send_commands = self.send_commands.entry(
            friend_public_key.clone()).or_insert(FriendSendCommands::new());
        friend_send_commands.try_send = true;
    }

    pub fn set_resend_outgoing(&mut self, friend_public_key: &PublicKey) {
        let friend_send_commands = self.send_commands.entry(
            friend_public_key.clone()).or_insert(FriendSendCommands::new());
        friend_send_commands.resend_outgoing = true;
    }

    pub fn set_remote_wants_token(&mut self, friend_public_key: &PublicKey) {
        let friend_send_commands = self.send_commands.entry(
            friend_public_key.clone()).or_insert(FriendSendCommands::new());
        friend_send_commands.remote_wants_token = true;
    }

    pub fn set_local_reset(&mut self, friend_public_key: &PublicKey) {
        let friend_send_commands = self.send_commands.entry(
            friend_public_key.clone()).or_insert(FriendSendCommands::new());
        friend_send_commands.local_reset = true;
    }
}


#[derive(Debug)]
enum PendingQueueError {
    InsufficientTrust,
    FreezeGuardBlock,
    MaxOperationsReached,
}

#[derive(Debug)]
enum CollectOutgoingError {
    MaxOperationsReached,
}

struct PendingMoveToken<A> {
    friend_public_key: PublicKey,
    outgoing_mc: OutgoingMc,
    operations: Vec<FriendTcOp>,
    opt_local_address: Option<A>,
    token_wanted: bool,
    max_operations_in_batch: usize,
}

impl<A> PendingMoveToken<A> 
where
    A: CanonicalSerialize + Clone,
{
    fn new(friend_public_key: PublicKey,
           outgoing_mc: OutgoingMc,
           max_operations_in_batch: usize) -> Self {

        PendingMoveToken {
            friend_public_key,
            outgoing_mc,
            operations: Vec::new(),
            opt_local_address: None,
            token_wanted: false,
            max_operations_in_batch,
        }
    }

    /// Attempt to queue one operation into a certain `pending_move_token`.
    /// If successful, mutations are applied and the operation is queued.
    /// Otherwise, an error is returned.
    fn queue_operation(&mut self, 
                       operation: &FriendTcOp,
                       m_state: &mut MutableFunderState<A>,
                       m_ephemeral: &mut MutableEphemeral)
        -> Result<(), PendingQueueError> 
    where
        A: Clone,
    {

        if self.operations.len() >= self.max_operations_in_batch {
            return Err(PendingQueueError::MaxOperationsReached);
        }

        // Freeze guard check (Only for requests):
        if let FriendTcOp::RequestSendFunds(request_send_funds) = operation {
            let verify_res = m_ephemeral
                .ephemeral()
                .freeze_guard
                .verify_freezing_links(&request_send_funds.route,
                                        request_send_funds.dest_payment,
                                       &request_send_funds.freeze_links);
            if verify_res.is_none() {
                return Err(PendingQueueError::FreezeGuardBlock);
            }
        }

        let mc_mutations = match self.outgoing_mc.queue_operation(operation) {
            Ok(mc_mutations) => Ok(mc_mutations),
            Err(QueueOperationError::RequestAlreadyExists) => {
                warn!("Request already exists: {:?}", operation);
                Ok(vec![])
            },
            Err(QueueOperationError::InsufficientTrust) => 
                Err(PendingQueueError::InsufficientTrust),
            Err(_) => unreachable!(),
        }?;

        // Update freeze guard here (Only for requests):
        if let FriendTcOp::RequestSendFunds(request_send_funds) = operation {
            let pending_request = &create_pending_request(&request_send_funds);

            let freeze_guard_mutation = FreezeGuardMutation::AddFrozenCredit(
                (pending_request.route.clone(), pending_request.dest_payment));
            let ephemeral_mutation = EphemeralMutation::FreezeGuardMutation(freeze_guard_mutation);
            m_ephemeral.mutate(ephemeral_mutation);
        }

        // Apply mutations:
        for mc_mutation in mc_mutations {
            let tc_mutation = TcMutation::McMutation(mc_mutation);
            let friend_mutation = FriendMutation::TcMutation(tc_mutation);
            let funder_mutation = FunderMutation::FriendMutation((self.friend_public_key.clone(), friend_mutation));
            m_state.mutate(funder_mutation);
        }

        Ok(())
    }

    /// Set local address inside pending move token.
    fn set_local_address(&mut self, 
                         local_address: A) {

        self.opt_local_address = Some(local_address);
    }
}

fn transmit_outgoing<A>(m_state: &MutableFunderState<A>,
                        friend_public_key: &PublicKey,
                        token_wanted: bool,
                        outgoing_messages: &mut Vec<OutgoingMessage<A>>)
where
    A: CanonicalSerialize + Clone,
{

    let friend = m_state.state().friends.get(friend_public_key).unwrap();
    let token_channel = match &friend.channel_status {
        ChannelStatus::Consistent(token_channel) => token_channel,
        ChannelStatus::Inconsistent(_) => unreachable!(),
    };

    let move_token = match &token_channel.get_direction() {
        TcDirection::Outgoing(tc_outgoing) => tc_outgoing.create_outgoing_move_token(),
        TcDirection::Incoming(_) => unreachable!(),
    };

    let move_token_request = MoveTokenRequest {
        friend_move_token: move_token,
        token_wanted,
    };

    outgoing_messages.push(
        (friend_public_key.clone(),
            FriendMessage::MoveTokenRequest(move_token_request)));
}

pub async fn apply_local_reset<'a,A,R>(m_state: &'a mut MutableFunderState<A>, 
                                  friend_public_key: &'a PublicKey,
                                  channel_inconsistent: &'a ChannelInconsistent,
                                  identity_client: &'a mut IdentityClient,
                                  rng: &'a R) 
where
    A: CanonicalSerialize + Clone + 'a,
    R: CryptoRandom,
{

    // TODO: How to do this without unwrap?:
    let remote_reset_terms = channel_inconsistent.opt_remote_reset_terms
        .clone()
        .unwrap();

    let rand_nonce = RandValue::new(rng);
    let move_token_counter = 0;

    let local_pending_debt = 0;
    let remote_pending_debt = 0;
    let opt_local_address = None;
    let u_reset_move_token = create_unsigned_move_token(
        // No operations are required for a reset move token
        Vec::new(), 
        opt_local_address,
        remote_reset_terms.reset_token.clone(),
        m_state.state().local_public_key.clone(),
        friend_public_key.clone(),
        remote_reset_terms.inconsistency_counter,
        move_token_counter,
        remote_reset_terms.balance_for_reset.checked_neg().unwrap(),
        local_pending_debt,
        remote_pending_debt,
        rand_nonce);

    let reset_move_token = await!(sign_move_token(u_reset_move_token, identity_client));

    let token_channel = TokenChannel::new_from_local_reset(
        &m_state.state().local_public_key,
        friend_public_key,
        &reset_move_token,
        remote_reset_terms.balance_for_reset,
        channel_inconsistent.opt_last_incoming_move_token.clone());

    let friend_mutation = FriendMutation::SetConsistent(token_channel);
    let funder_mutation = FunderMutation::FriendMutation((friend_public_key.clone(), friend_mutation));
    m_state.mutate(funder_mutation);

}

async fn send_friend_iter1<'a,A,R>(m_state: &'a mut MutableFunderState<A>,
                                       m_ephemeral: &'a mut MutableEphemeral,
                                       friend_public_key: &'a PublicKey, 
                                       friend_send_commands: &'a FriendSendCommands, 
                                       pending_move_tokens: &'a mut HashMap<PublicKey, PendingMoveToken<A>>,
                                       identity_client: &'a mut IdentityClient,
                                       rng: &'a R,
                                       max_operations_in_batch: usize,
                                       mut outgoing_messages: &'a mut Vec<OutgoingMessage<A>>) 
where
    A: CanonicalSerialize + Clone + Eq,
    R: CryptoRandom,
{

    if !friend_send_commands.try_send 
        && !friend_send_commands.resend_outgoing 
        && !friend_send_commands.remote_wants_token
        && !friend_send_commands.local_reset {

        return;
    }

    let friend = m_state.state().friends.get(friend_public_key).unwrap();

    // Check if we need to perform a local reset:
    if friend_send_commands.local_reset {
        if let ChannelStatus::Inconsistent(channel_inconsistent) = 
            &friend.channel_status {
            let c_channel_inconsistent = channel_inconsistent.clone();
            await!(apply_local_reset(m_state,
                                     friend_public_key,
                                     &c_channel_inconsistent,
                                     identity_client,
                                     rng));
        }
    }

    let friend = m_state.state().friends.get(friend_public_key).unwrap();

    let token_channel = match &friend.channel_status {
        ChannelStatus::Consistent(token_channel) => token_channel,
        ChannelStatus::Inconsistent(channel_inconsistent) => {
            if friend_send_commands.resend_outgoing {
                outgoing_messages.push((friend_public_key.clone(),
                        FriendMessage::InconsistencyError(channel_inconsistent.local_reset_terms.clone())));
            }
            return;
        },
    };

    let tc_incoming = match token_channel.get_direction() {
        TcDirection::Outgoing(_) => {
            if estimate_should_send(m_state, friend_public_key) {
                let is_token_wanted = true;
                transmit_outgoing(m_state,
                                  &friend_public_key,
                                  is_token_wanted,
                                  &mut outgoing_messages);
            } else {
                if friend_send_commands.resend_outgoing {
                    let is_token_wanted = false;
                    transmit_outgoing(m_state,
                                      &friend_public_key,
                                      is_token_wanted,
                                      &mut outgoing_messages);
                }
            }
            return;
        },
        TcDirection::Incoming(tc_incoming) => tc_incoming,
    };

    // If we are here, the token channel is incoming:

    // It will be strange if we need to resend outgoing, because the channel
    // is in incoming mode.
    // -- This could happen in handle_liveness.
    // assert!(!friend_send_commands.resend_outgoing);

    let outgoing_mc = tc_incoming.begin_outgoing_move_token();
    let pending_move_token = PendingMoveToken::new(friend_public_key.clone(), 
                                                   outgoing_mc,
                                                   max_operations_in_batch);
    pending_move_tokens.insert(friend_public_key.clone(), pending_move_token);
    let pending_move_token = pending_move_tokens.get_mut(friend_public_key).unwrap();
    let _ = await!(collect_outgoing_move_token(m_state, 
                                               m_ephemeral, 
                                               friend_public_key, 
                                               pending_move_token,
                                               identity_client,
                                               rng));
}


/// Do we need to send anything to the remote side?
/// Note that this is only an estimation. It is possible that when the token from remote side
/// arrives, the state will be different.
fn estimate_should_send<'a, A>(m_state: &'a mut MutableFunderState<A>, 
                            friend_public_key: &'a PublicKey) -> bool 
where
    A: CanonicalSerialize + Clone + Eq,
{

    let friend = m_state.state().friends.get(friend_public_key).unwrap();

    // Check if notification about local address change is required:
    if let Some(local_address) = &m_state.state().opt_address {
        let friend = m_state.state().friends.get(friend_public_key).unwrap();
        match &friend.sent_local_address {
            SentLocalAddress::NeverSent => return true,
            SentLocalAddress::Transition((last_address, _)) |
            SentLocalAddress::LastSent(last_address) => {
                if last_address != local_address {
                    return true;
                }
            }
        };
    }

    // Check if update to remote_max_debt is required:
    match &friend.channel_status {
        ChannelStatus::Consistent(token_channel) => {
            if friend.wanted_remote_max_debt != token_channel.get_remote_max_debt() {
                return true;
            }

            // Open or close requests is needed:
            let local_requests_status = &token_channel
                .get_mutual_credit()
                .state()
                .requests_status
                .local;

            if friend.wanted_local_requests_status != *local_requests_status {
                return true;
            }
        },
        ChannelStatus::Inconsistent(_) => {},
    };

    if !friend.pending_responses.is_empty() {
        return true;
    }

    if !friend.pending_requests.is_empty() {
        return true;
    }

    if !friend.pending_user_requests.is_empty() {
        return true;
    }

    false
}

async fn queue_operation_or_failure<'a,A>(m_state: &'a mut MutableFunderState<A>,
                                            m_ephemeral: &'a mut MutableEphemeral,
                                            pending_move_token: &'a mut PendingMoveToken<A>,
                                            operation: &'a FriendTcOp) -> Result<(), CollectOutgoingError> 
where
    A: CanonicalSerialize + Clone,
{

    match pending_move_token.queue_operation(operation, m_state, m_ephemeral) {
        Ok(()) => return Ok(()),
        Err(PendingQueueError::MaxOperationsReached) => {
            pending_move_token.token_wanted = true;
            // We will send this message next time we have the token:
            return Err(CollectOutgoingError::MaxOperationsReached);
        }
        Err(PendingQueueError::InsufficientTrust) |
        Err(PendingQueueError::FreezeGuardBlock) => {},
    };

    // The operation must have been a request if we had one of the above errors:
    let request_send_funds = match operation {
        FriendTcOp::RequestSendFunds(request_send_funds) => 
            request_send_funds,
        _ => unreachable!(),
    };

    // We are here if an error occured. 
    // We cancel the request:
    let pending_request = create_pending_request(request_send_funds);
    let u_failure_op = ResponseOp::UnsignedFailure(pending_request);
    let friend_mutation = FriendMutation::PushBackPendingResponse(u_failure_op);
    let funder_mutation = FunderMutation::FriendMutation((pending_move_token.friend_public_key.clone(), friend_mutation));
    m_state.mutate(funder_mutation);

    Ok(())
}

async fn response_op_to_friend_tc_op<'a,A,R>(m_state: &'a mut MutableFunderState<A>, 
                                     response_op: ResponseOp,
                                     mut identity_client: &'a mut IdentityClient,
                                     rng: &'a R) -> FriendTcOp 
where
    A: CanonicalSerialize + Clone,
    R: CryptoRandom,
{
    match response_op {
        ResponseOp::Response(response) => FriendTcOp::ResponseSendFunds(response),
        ResponseOp::UnsignedResponse(pending_request) => {
            let rand_nonce = RandValue::new(rng);
            FriendTcOp::ResponseSendFunds(await!(create_response_send_funds(
                        &pending_request, rand_nonce, identity_client)))
        },
        ResponseOp::Failure(failure) => FriendTcOp::FailureSendFunds(failure),
        ResponseOp::UnsignedFailure(pending_request) => {
            let rand_nonce = RandValue::new(rng);
            FriendTcOp::FailureSendFunds(await!(create_failure_send_funds(
                        &pending_request, &(m_state.state().local_public_key), 
                        rand_nonce, &mut identity_client)))
        }
    }
}

/// Given a friend with an incoming move token state, create the largest possible move token to
/// send to the remote side. 
/// Requests that fail to be processed are moved to the failure queues of the relevant friends.
async fn collect_outgoing_move_token<'a,A,R>(m_state: &'a mut MutableFunderState<A>,
                                                 m_ephemeral: &'a mut MutableEphemeral,
                                                 friend_public_key: &'a PublicKey,
                                                 pending_move_token: &'a mut PendingMoveToken<A>,
                                                 identity_client: &'a mut IdentityClient,
                                                 rng: &'a R) 
                                                    -> Result<(), CollectOutgoingError> 
where
    A: CanonicalSerialize + Clone + Eq,
    R: CryptoRandom,
{
    /*
    - Check if last sent local address is up to date.
    - Collect as many operations as possible (Not more than max ops per batch)
        1. Responses (response, failure)
        2. Pending requets
        3. User pending requests
    - When adding requests, check the following:
        - Valid by freezeguard.
        - Valid from credits point of view.
    - If a request is not valid, Pass it as a failure message to
        relevant friend.
    */

    // Send update about local address if needed:
    let friend = m_state.state().friends.get(friend_public_key).unwrap();
    let opt_new_sent_local_address = if let Some(local_address) = &m_state.state().opt_address {
        match &friend.sent_local_address {
            SentLocalAddress::NeverSent => {
                pending_move_token.set_local_address(local_address.clone());
                Some(SentLocalAddress::LastSent(local_address.clone()))
            },
            SentLocalAddress::Transition((last_sent_local_address, _)) |
            SentLocalAddress::LastSent(last_sent_local_address) => {
                if local_address != last_sent_local_address {
                    pending_move_token.set_local_address(local_address.clone());
                    Some(SentLocalAddress::Transition((local_address.clone(), last_sent_local_address.clone())))
                } else {
                    None
                }
            },
        }
    } else {
        None
    };

    // Update friend.sent_local_address accordingly:
    if let Some(new_sent_local_address) = opt_new_sent_local_address {
        let friend_mutation = FriendMutation::SetSentLocalAddress(new_sent_local_address);
        let funder_mutation = FunderMutation::FriendMutation((friend_public_key.clone(), friend_mutation));
        m_state.mutate(funder_mutation);
    }

    let friend = m_state.state().friends.get(friend_public_key).unwrap();

    // Set remote_max_debt if needed:
    let remote_max_debt = match &friend.channel_status {
        ChannelStatus::Consistent(token_channel) => token_channel,
        ChannelStatus::Inconsistent(_) => unreachable!(),
    }.get_remote_max_debt();


    if friend.wanted_remote_max_debt != remote_max_debt {
        let operation = FriendTcOp::SetRemoteMaxDebt(friend.wanted_remote_max_debt);
        await!(queue_operation_or_failure(m_state,
                                          m_ephemeral,
                                          pending_move_token,
                                          &operation))?;
    }

    let friend = m_state.state().friends.get(friend_public_key).unwrap();
    let token_channel = match &friend.channel_status {
        ChannelStatus::Consistent(token_channel) => token_channel,
        ChannelStatus::Inconsistent(_) => unreachable!(),
    };

    // Open or close requests is needed:
    let local_requests_status = &token_channel
        .get_mutual_credit()
        .state()
        .requests_status
        .local;

    if friend.wanted_local_requests_status != *local_requests_status {
        let friend_op = if let RequestsStatus::Open = friend.wanted_local_requests_status {
            FriendTcOp::EnableRequests
        } else {
            FriendTcOp::DisableRequests
        };
        await!(queue_operation_or_failure(m_state,
                                          m_ephemeral,
                                          pending_move_token,
                                          &friend_op))?;
    }

    let friend = m_state.state().friends.get(friend_public_key).unwrap();
    // Send pending responses (responses and failures)
    // TODO: Possibly replace this clone with something more efficient later:
    let mut pending_responses = friend.pending_responses.clone();
    while let Some(pending_response) = pending_responses.pop_front() {
        let pending_op = await!(response_op_to_friend_tc_op(m_state, pending_response,
                                                            identity_client, rng));
        await!(queue_operation_or_failure(m_state,
                                          m_ephemeral,
                                          pending_move_token,
                                          &pending_op))?;

        let friend_mutation = FriendMutation::PopFrontPendingResponse;
        let funder_mutation = FunderMutation::FriendMutation((friend_public_key.clone(), friend_mutation));
        m_state.mutate(funder_mutation);
    }

    let friend = m_state.state().friends.get(friend_public_key).unwrap();

    // Send pending requests:
    // TODO: Possibly replace this clone with something more efficient later:
    let mut pending_requests = friend.pending_requests.clone();
    while let Some(pending_request) = pending_requests.pop_front() {
        let pending_op = FriendTcOp::RequestSendFunds(pending_request);
        await!(queue_operation_or_failure(m_state,
                                          m_ephemeral,
                                          pending_move_token,
                                          &pending_op))?;
        let friend_mutation = FriendMutation::PopFrontPendingRequest;
        let funder_mutation = FunderMutation::FriendMutation((friend_public_key.clone(), friend_mutation));
        m_state.mutate(funder_mutation);
    }

    let friend = m_state.state().friends.get(friend_public_key).unwrap();

    // Send as many pending user requests as possible:
    let mut pending_user_requests = friend.pending_user_requests.clone();
    while let Some(request_send_funds) = pending_user_requests.pop_front() {
        let request_op = FriendTcOp::RequestSendFunds(request_send_funds);
        await!(queue_operation_or_failure(m_state,
                                          m_ephemeral,
                                          pending_move_token,
                                          &request_op))?;
        let friend_mutation = FriendMutation::PopFrontPendingUserRequest;
        let funder_mutation = FunderMutation::FriendMutation((friend_public_key.clone(), friend_mutation));
        m_state.mutate(funder_mutation);
    }
    Ok(())
}



async fn append_failures_to_move_token<'a,A,R>(m_state: &'a mut MutableFunderState<A>,
                                                   m_ephemeral: &'a mut MutableEphemeral,
                                                   friend_public_key: &'a PublicKey,
                                                   pending_move_token: &'a mut PendingMoveToken<A>,
                                                   identity_client: &'a mut IdentityClient,
                                                   rng: &'a R) 
                                                    -> Result<(), CollectOutgoingError> 
where
    A: CanonicalSerialize + Clone,
    R: CryptoRandom,
{

    let friend = m_state.state().friends.get(friend_public_key).unwrap();

    // Send pending responses (responses and failures)
    // TODO: Possibly replace this clone with something more efficient later:
    let mut pending_responses = friend.pending_responses.clone();
    while let Some(pending_response) = pending_responses.pop_front() {
        let pending_op = await!(response_op_to_friend_tc_op(m_state, 
                                                            pending_response,
                                                            identity_client,
                                                            rng));
        await!(queue_operation_or_failure(m_state,
                                          m_ephemeral,
                                          pending_move_token,
                                          &pending_op))?;

        let friend_mutation = FriendMutation::PopFrontPendingResponse;
        let funder_mutation = FunderMutation::FriendMutation((friend_public_key.clone(), friend_mutation));
        m_state.mutate(funder_mutation);
    }
    Ok(())
}

async fn send_move_token<'a,A,R>(m_state: &'a mut MutableFunderState<A>,
                                 friend_public_key: PublicKey,
                                 pending_move_token: PendingMoveToken<A>,
                                 identity_client: &'a mut IdentityClient,
                                 rng: &'a R,
                                 outgoing_messages: &'a mut Vec<OutgoingMessage<A>>) 
where
    A: Clone + CanonicalSerialize + 'a,
    R: CryptoRandom,
{

    let PendingMoveToken {
        operations,
        opt_local_address,
        token_wanted,
        ..
    } = pending_move_token;

    let friend = m_state.state().friends.get(&friend_public_key).unwrap();

    let rand_nonce = RandValue::new(rng);
    let token_channel = match &friend.channel_status {
        ChannelStatus::Consistent(token_channel) => token_channel,
        ChannelStatus::Inconsistent(_) => unreachable!(),
    };

    let tc_incoming = match token_channel.get_direction() {
        TcDirection::Outgoing(_) => unreachable!(),
        TcDirection::Incoming(tc_incoming) => tc_incoming,
    };

    let u_move_token = tc_incoming.create_unsigned_move_token(operations, 
                                         opt_local_address,
                                         rand_nonce);

    let move_token = await!(sign_move_token(u_move_token, identity_client));

    let tc_mutation = TcMutation::SetDirection(
        SetDirection::Outgoing(move_token));
    let friend_mutation = FriendMutation::TcMutation(tc_mutation);
    let funder_mutation = FunderMutation::FriendMutation((friend_public_key.clone(), friend_mutation));
    m_state.mutate(funder_mutation);

    let friend = m_state.state().friends.get(&friend_public_key).unwrap();
    let token_channel = match &friend.channel_status {
        ChannelStatus::Consistent(token_channel) => token_channel,
        ChannelStatus::Inconsistent(_) => unreachable!(),
    };

    let tc_outgoing = match token_channel.get_direction() {
        TcDirection::Outgoing(tc_outgoing) => tc_outgoing,
        TcDirection::Incoming(_) => unreachable!(),
    };

    let friend_move_token = tc_outgoing.create_outgoing_move_token();
    let move_token_request = MoveTokenRequest {
        friend_move_token,
        token_wanted,
    };

    outgoing_messages.push((friend_public_key.clone(),
            FriendMessage::MoveTokenRequest(move_token_request)));
}


/// Send all possible messages according to SendCommands
pub async fn create_friend_messages<'a,A,R>(m_state: &'a mut MutableFunderState<A>, 
                        m_ephemeral: &'a mut MutableEphemeral,
                        send_commands: &'a SendCommands,
                        max_operations_in_batch: usize,
                        identity_client: &'a mut IdentityClient,
                        rng: &'a R) -> Vec<OutgoingMessage<A>> 
where
    A: CanonicalSerialize + Clone + Eq,
    R: CryptoRandom,
{

    let mut outgoing_messages = Vec::new();
    let mut pending_move_tokens: HashMap<PublicKey, PendingMoveToken<A>> 
        = HashMap::new();

    // First iteration:
    for (friend_public_key, friend_send_commands) in &send_commands.send_commands {
        await!(send_friend_iter1(m_state, 
                                 m_ephemeral,
                                 friend_public_key,
                                 friend_send_commands,
                                 &mut pending_move_tokens,
                                 identity_client,
                                 rng,
                                 max_operations_in_batch,
                                 &mut outgoing_messages));
    }

    // Second iteration (Attempt to queue failures created in the first iteration):
    for (friend_public_key, pending_move_token) in &mut pending_move_tokens {
        let _ = await!(append_failures_to_move_token(m_state,
                                                     m_ephemeral,
                                                     friend_public_key, 
                                                     pending_move_token,
                                                     identity_client,
                                                     rng));
    }

    // Send all pending move tokens:
    for (friend_public_key, pending_move_token) in pending_move_tokens.into_iter() {
        await!(send_move_token(m_state, 
                               friend_public_key, 
                               pending_move_token, 
                               identity_client,
                               rng,
                               &mut outgoing_messages));
    }

    outgoing_messages
}



