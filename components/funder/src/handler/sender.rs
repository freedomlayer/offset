use std::collections::{HashMap, HashSet};
use std::fmt::Debug;

use common::canonical_serialize::CanonicalSerialize;

use crypto::crypto_rand::{CryptoRandom, RandValue};
use crypto::identity::PublicKey;

use proto::app_server::messages::RelayAddress;
use proto::funder::messages::{
    ChannelerUpdateFriend, FriendMessage, FriendTcOp, FunderOutgoingControl, MoveTokenRequest,
    RequestResult, RequestsStatus, TransactionResult,
};

use identity::IdentityClient;

use crate::mutual_credit::outgoing::{OutgoingMc, QueueOperationError};
use crate::types::{
    create_cancel_send_funds, create_pending_transaction, create_unsigned_move_token,
    sign_move_token, ChannelerConfig,
};

use crate::friend::{
    BackwardsOp, ChannelInconsistent, ChannelStatus, FriendMutation, SentLocalRelays,
};
use crate::token_channel::{SetDirection, TcDirection, TcMutation, TokenChannel};

use crate::ephemeral::Ephemeral;
use crate::handler::state_wrap::MutableFunderState;
use crate::handler::utils::find_request_origin;
use crate::state::{FunderMutation, FunderState};

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

pub type OutgoingMessage<B> = (PublicKey, FriendMessage<B>);

#[derive(Clone)]
pub struct SendCommands {
    pub send_commands: HashMap<PublicKey, FriendSendCommands>,
}

impl SendCommands {
    pub fn new() -> Self {
        SendCommands {
            send_commands: HashMap::new(),
        }
    }

    pub fn set_try_send(&mut self, friend_public_key: &PublicKey) {
        let friend_send_commands = self
            .send_commands
            .entry(friend_public_key.clone())
            .or_insert_with(FriendSendCommands::new);
        friend_send_commands.try_send = true;
    }

    pub fn set_resend_outgoing(&mut self, friend_public_key: &PublicKey) {
        let friend_send_commands = self
            .send_commands
            .entry(friend_public_key.clone())
            .or_insert_with(FriendSendCommands::new);
        friend_send_commands.resend_outgoing = true;
    }

    pub fn set_remote_wants_token(&mut self, friend_public_key: &PublicKey) {
        let friend_send_commands = self
            .send_commands
            .entry(friend_public_key.clone())
            .or_insert_with(FriendSendCommands::new);
        friend_send_commands.remote_wants_token = true;
    }

    pub fn set_local_reset(&mut self, friend_public_key: &PublicKey) {
        let friend_send_commands = self
            .send_commands
            .entry(friend_public_key.clone())
            .or_insert_with(FriendSendCommands::new);
        friend_send_commands.local_reset = true;
    }
}

#[derive(Debug)]
enum PendingQueueError {
    InsufficientTrust,
    MaxOperationsReached,
}

#[derive(Debug)]
enum CollectOutgoingError {
    MaxOperationsReached,
}

struct PendingMoveToken<B> {
    friend_public_key: PublicKey,
    outgoing_mc: OutgoingMc,
    operations: Vec<FriendTcOp>,
    opt_local_relays: Option<Vec<RelayAddress<B>>>,
    token_wanted: bool,
    max_operations_in_batch: usize,
    /// Can we send this move token with empty operations list
    /// and empty opt_local_address?
    may_send_empty: bool,
}

impl<B> PendingMoveToken<B>
where
    B: Clone + CanonicalSerialize + PartialEq + Eq + Debug,
{
    fn new(
        friend_public_key: PublicKey,
        outgoing_mc: OutgoingMc,
        max_operations_in_batch: usize,
        may_send_empty: bool,
    ) -> Self {
        PendingMoveToken {
            friend_public_key,
            outgoing_mc,
            operations: Vec::new(),
            opt_local_relays: None,
            token_wanted: false,
            max_operations_in_batch,
            may_send_empty,
        }
    }

    /// Attempt to queue one operation into a certain `pending_move_token`.
    /// If successful, mutations are applied and the operation is queued.
    /// Otherwise, an error is returned.
    fn queue_operation(
        &mut self,
        operation: &FriendTcOp,
        m_state: &mut MutableFunderState<B>,
    ) -> Result<(), PendingQueueError> {
        if self.operations.len() >= self.max_operations_in_batch {
            return Err(PendingQueueError::MaxOperationsReached);
        }

        let mc_mutations = match self.outgoing_mc.queue_operation(operation) {
            Ok(mc_mutations) => Ok(mc_mutations),
            Err(QueueOperationError::RequestAlreadyExists) => {
                warn!("Request already exists: {:?}", operation);
                Ok(vec![])
            }
            Err(QueueOperationError::InsufficientTrust) => {
                Err(PendingQueueError::InsufficientTrust)
            }
            Err(_) => unreachable!(),
        }?;

        // Add operation:
        self.operations.push(operation.clone());

        // Apply mutations:
        for mc_mutation in mc_mutations {
            let tc_mutation = TcMutation::McMutation(mc_mutation);
            let friend_mutation = FriendMutation::TcMutation(tc_mutation);
            let funder_mutation =
                FunderMutation::FriendMutation((self.friend_public_key.clone(), friend_mutation));
            m_state.mutate(funder_mutation);
        }

        Ok(())
    }

    /// Set local address inside pending move token.
    fn set_local_relays(&mut self, local_relays: Vec<RelayAddress<B>>) {
        self.opt_local_relays = Some(local_relays);
    }
}

fn transmit_outgoing<B>(
    m_state: &MutableFunderState<B>,
    friend_public_key: &PublicKey,
    token_wanted: bool,
    outgoing_messages: &mut Vec<OutgoingMessage<B>>,
) where
    B: Clone + CanonicalSerialize + PartialEq + Eq + Debug,
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

    outgoing_messages.push((
        friend_public_key.clone(),
        FriendMessage::MoveTokenRequest(move_token_request),
    ));
}

pub async fn apply_local_reset<'a, B, R>(
    m_state: &'a mut MutableFunderState<B>,
    friend_public_key: &'a PublicKey,
    channel_inconsistent: &'a ChannelInconsistent,
    identity_client: &'a mut IdentityClient,
    rng: &'a R,
) where
    B: Clone + CanonicalSerialize + PartialEq + Eq + Debug,
    R: CryptoRandom,
{
    // TODO: How to do this without unwrap?:
    let remote_reset_terms = channel_inconsistent.opt_remote_reset_terms.clone().unwrap();

    let rand_nonce = RandValue::new(rng);
    let move_token_counter = 0;

    let local_pending_debt = 0;
    let remote_pending_debt = 0;
    let opt_local_relays = None;
    let u_reset_move_token = create_unsigned_move_token(
        // No operations are required for a reset move token
        Vec::new(),
        opt_local_relays,
        remote_reset_terms.reset_token.clone(),
        m_state.state().local_public_key.clone(),
        friend_public_key.clone(),
        remote_reset_terms.inconsistency_counter,
        move_token_counter,
        remote_reset_terms.balance_for_reset.checked_neg().unwrap(),
        local_pending_debt,
        remote_pending_debt,
        rand_nonce,
    );

    let reset_move_token = await!(sign_move_token(u_reset_move_token, identity_client));

    let token_channel = TokenChannel::new_from_local_reset(
        &m_state.state().local_public_key,
        friend_public_key,
        &reset_move_token,
        remote_reset_terms.balance_for_reset.checked_neg().unwrap(),
        channel_inconsistent.opt_last_incoming_move_token.clone(),
    );

    let friend_mutation = FriendMutation::SetConsistent(token_channel);
    let funder_mutation =
        FunderMutation::FriendMutation((friend_public_key.clone(), friend_mutation));
    m_state.mutate(funder_mutation);
}

async fn send_friend_iter1<'a, B, R>(
    m_state: &'a mut MutableFunderState<B>,
    friend_public_key: &'a PublicKey,
    friend_send_commands: &'a FriendSendCommands,
    pending_move_tokens: &'a mut HashMap<PublicKey, PendingMoveToken<B>>,
    identity_client: &'a mut IdentityClient,
    rng: &'a R,
    max_operations_in_batch: usize,
    cancel_public_keys: &'a mut HashSet<PublicKey>,
    mut outgoing_messages: &'a mut Vec<OutgoingMessage<B>>,
    outgoing_control: &'a mut Vec<FunderOutgoingControl<B>>,
    outgoing_channeler_config: &'a mut Vec<ChannelerConfig<RelayAddress<B>>>,
) where
    B: Clone + PartialEq + Eq + CanonicalSerialize + Debug,
    R: CryptoRandom,
{
    if !friend_send_commands.try_send
        && !friend_send_commands.resend_outgoing
        && !friend_send_commands.remote_wants_token
        && !friend_send_commands.local_reset
    {
        return;
    }

    let friend = m_state.state().friends.get(friend_public_key).unwrap();

    // Check if we need to perform a local reset:
    if friend_send_commands.local_reset {
        if let ChannelStatus::Inconsistent(channel_inconsistent) = &friend.channel_status {
            let c_channel_inconsistent = channel_inconsistent.clone();
            await!(apply_local_reset(
                m_state,
                friend_public_key,
                &c_channel_inconsistent,
                identity_client,
                rng
            ));
        }
    }

    let friend = m_state.state().friends.get(friend_public_key).unwrap();

    let token_channel = match &friend.channel_status {
        ChannelStatus::Consistent(token_channel) => token_channel,
        ChannelStatus::Inconsistent(channel_inconsistent) => {
            if friend_send_commands.resend_outgoing || friend_send_commands.try_send {
                outgoing_messages.push((
                    friend_public_key.clone(),
                    FriendMessage::InconsistencyError(
                        channel_inconsistent.local_reset_terms.clone(),
                    ),
                ));
            }
            return;
        }
    };

    let tc_incoming = match &token_channel.get_direction() {
        TcDirection::Outgoing(tc_outgoing) => {
            if estimate_should_send(m_state.state(), friend_public_key) {
                let is_token_wanted = true;
                transmit_outgoing(
                    m_state,
                    &friend_public_key,
                    is_token_wanted,
                    &mut outgoing_messages,
                );
            } else if friend_send_commands.resend_outgoing {
                let is_token_wanted = tc_outgoing.move_token_out.opt_local_relays.is_some();
                transmit_outgoing(
                    m_state,
                    &friend_public_key,
                    is_token_wanted,
                    &mut outgoing_messages,
                );
            }

            return;
        }
        TcDirection::Incoming(tc_incoming) => tc_incoming,
    };

    // If we are here, the token channel is incoming:

    // It will be strange if we need to resend outgoing, because the channel
    // is in incoming mode.
    // -- This could happen in handle_liveness.
    // assert!(!friend_send_commands.resend_outgoing);

    let outgoing_mc = tc_incoming.begin_outgoing_move_token();
    let may_send_empty =
        friend_send_commands.resend_outgoing || friend_send_commands.remote_wants_token;
    let pending_move_token = PendingMoveToken::new(
        friend_public_key.clone(),
        outgoing_mc,
        max_operations_in_batch,
        may_send_empty,
    );
    pending_move_tokens.insert(friend_public_key.clone(), pending_move_token);
    let pending_move_token = pending_move_tokens.get_mut(friend_public_key).unwrap();
    let _ = collect_outgoing_move_token(
        m_state,
        outgoing_channeler_config,
        outgoing_control,
        cancel_public_keys,
        friend_public_key,
        pending_move_token,
    );
}

/// Do we need to send anything to the remote side?
/// Note that this is only an estimation. It is possible that when the token from remote side
/// arrives, the state will be different.
fn estimate_should_send<'a, B>(state: &'a FunderState<B>, friend_public_key: &'a PublicKey) -> bool
where
    B: Clone + PartialEq + Eq + CanonicalSerialize + Debug,
{
    // Check if notification about local address change is required:
    let friend = state.friends.get(friend_public_key).unwrap();
    match &friend.sent_local_relays {
        SentLocalRelays::NeverSent => return true,
        SentLocalRelays::Transition((relays, _)) | SentLocalRelays::LastSent(relays) => {
            if relays != &state.relays {
                return true;
            }
        }
    };

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
        }
        ChannelStatus::Inconsistent(_) => {}
    };

    if !friend.pending_backwards_ops.is_empty() {
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

/// Queue an operation to a PendingMoveToken.
/// On cancel, queue a Cancel message to the relevant friend,
/// or (if we are the origin of the request): send a failure notification through the control.
fn queue_operation_or_cancel<'a, B>(
    m_state: &'a mut MutableFunderState<B>,
    pending_move_token: &'a mut PendingMoveToken<B>,
    cancel_public_keys: &'a mut HashSet<PublicKey>,
    outgoing_control: &'a mut Vec<FunderOutgoingControl<B>>,
    operation: &'a FriendTcOp,
) -> Result<(), CollectOutgoingError>
where
    B: Clone + CanonicalSerialize + PartialEq + Eq + Debug,
{
    match pending_move_token.queue_operation(operation, m_state) {
        Ok(()) => return Ok(()),
        Err(PendingQueueError::MaxOperationsReached) => {
            pending_move_token.token_wanted = true;
            // We will send this message next time we have the token:
            return Err(CollectOutgoingError::MaxOperationsReached);
        }
        Err(PendingQueueError::InsufficientTrust) => {}
    };

    // The operation must have been a request if we had one of the above errors:
    let request_send_funds = match operation {
        FriendTcOp::RequestSendFunds(request_send_funds) => request_send_funds,
        _ => unreachable!(),
    };

    // We are here if an error occurred.
    // We cancel the request:

    match find_request_origin(m_state.state(), &request_send_funds.request_id).cloned() {
        Some(origin_public_key) => {
            // The friend with public key `origin_public_key` is the origin of this request.
            // We send him back a Cancel message:
            let pending_transaction = create_pending_transaction(request_send_funds);
            let cancel_send_funds =
                BackwardsOp::Cancel(create_cancel_send_funds(pending_transaction.request_id));
            let friend_mutation = FriendMutation::PushBackPendingBackwardsOp(cancel_send_funds);
            let funder_mutation =
                FunderMutation::FriendMutation((origin_public_key.clone(), friend_mutation));
            m_state.mutate(funder_mutation);

            cancel_public_keys.insert(origin_public_key.clone());
        }
        None => {
            // We are the origin of this request
            let transaction_result = TransactionResult {
                request_id: request_send_funds.request_id,
                result: RequestResult::Failure,
            };
            outgoing_control.push(FunderOutgoingControl::TransactionResult(transaction_result));
        }
    }

    Ok(())
}

fn backwards_op_to_friend_tc_op(backwards_op: BackwardsOp) -> FriendTcOp {
    match backwards_op {
        BackwardsOp::Response(response_send_funds) => {
            FriendTcOp::ResponseSendFunds(response_send_funds)
        }
        BackwardsOp::Cancel(cancel_send_funds) => FriendTcOp::CancelSendFunds(cancel_send_funds),
        BackwardsOp::Collect(collect_send_funds) => {
            FriendTcOp::CollectSendFunds(collect_send_funds)
        }
    }
}

/// Given a friend with an incoming move token state, create the largest possible move token to
/// send to the remote side.
/// Requests that fail to be processed are moved to the cancel queues of the relevant friends.
fn collect_outgoing_move_token<'a, B>(
    m_state: &'a mut MutableFunderState<B>,
    outgoing_channeler_config: &'a mut Vec<ChannelerConfig<RelayAddress<B>>>,
    outgoing_control: &'a mut Vec<FunderOutgoingControl<B>>,
    cancel_public_keys: &'a mut HashSet<PublicKey>,
    friend_public_key: &'a PublicKey,
    pending_move_token: &'a mut PendingMoveToken<B>,
) -> Result<(), CollectOutgoingError>
where
    B: Clone + PartialEq + Eq + CanonicalSerialize + Debug,
{
    /*
    - Check if last sent local address is up to date.
    - Collect as many operations as possible (Not more than max ops per batch)
        1. Responses (response, cancel, collect)
        2. Pending requests
        3. User pending requests
    - When adding requests, check the following:
        - Valid from credits point of view.
    - If a request is not valid, queue a Cancel message to relevant friend.
    */

    // Send update about local address if needed:
    let friend = m_state.state().friends.get(friend_public_key).unwrap();
    let local_named_relays = m_state.state().relays.clone();

    let local_relays = local_named_relays
        .iter()
        .cloned()
        .map(RelayAddress::from)
        .collect();

    let opt_new_sent_local_relays = match &friend.sent_local_relays {
        SentLocalRelays::NeverSent => {
            pending_move_token.set_local_relays(local_relays);
            Some(SentLocalRelays::LastSent(local_named_relays.clone()))
        }
        SentLocalRelays::Transition((last_sent_local_relays, _))
        | SentLocalRelays::LastSent(last_sent_local_relays) => {
            if &local_named_relays != last_sent_local_relays {
                pending_move_token.set_local_relays(local_relays.clone());
                Some(SentLocalRelays::Transition((
                    local_named_relays.clone(),
                    last_sent_local_relays.clone(),
                )))
            } else {
                None
            }
        }
    };

    // Update friend.sent_local_relays accordingly:
    if let Some(new_sent_local_relays) = opt_new_sent_local_relays {
        let friend_mutation = FriendMutation::SetSentLocalRelays(new_sent_local_relays);
        let funder_mutation =
            FunderMutation::FriendMutation((friend_public_key.clone(), friend_mutation));
        m_state.mutate(funder_mutation);

        let friend = m_state.state().friends.get(friend_public_key).unwrap();

        // Notify Channeler to change the friend's address:
        let update_friend = ChannelerUpdateFriend {
            friend_public_key: friend_public_key.clone(),
            friend_relays: friend.remote_relays.clone(),
            local_relays: friend.sent_local_relays.to_vec(),
        };
        let channeler_config = ChannelerConfig::UpdateFriend(update_friend);
        outgoing_channeler_config.push(channeler_config);
    }

    let friend = m_state.state().friends.get(friend_public_key).unwrap();

    // Set remote_max_debt if needed:
    let remote_max_debt = match &friend.channel_status {
        ChannelStatus::Consistent(token_channel) => token_channel,
        ChannelStatus::Inconsistent(_) => unreachable!(),
    }
    .get_remote_max_debt();

    if friend.wanted_remote_max_debt != remote_max_debt {
        let operation = FriendTcOp::SetRemoteMaxDebt(friend.wanted_remote_max_debt);
        queue_operation_or_cancel(
            m_state,
            pending_move_token,
            cancel_public_keys,
            outgoing_control,
            &operation,
        )?;
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
        queue_operation_or_cancel(
            m_state,
            pending_move_token,
            cancel_public_keys,
            outgoing_control,
            &friend_op,
        )?;
    }

    let friend = m_state.state().friends.get(friend_public_key).unwrap();
    // Send pending responses (Response, Cancel, Collect)
    // TODO: Possibly replace this clone with something more efficient later:
    let mut pending_backwards_ops = friend.pending_backwards_ops.clone();
    while let Some(pending_backwards_op) = pending_backwards_ops.pop_front() {
        let pending_op = backwards_op_to_friend_tc_op(pending_backwards_op);
        queue_operation_or_cancel(
            m_state,
            pending_move_token,
            // Not required here, as no requests are being sent.
            cancel_public_keys,
            outgoing_control,
            &pending_op,
        )?;

        let friend_mutation = FriendMutation::PopFrontPendingBackwardsOp;
        let funder_mutation =
            FunderMutation::FriendMutation((friend_public_key.clone(), friend_mutation));
        m_state.mutate(funder_mutation);
    }

    let friend = m_state.state().friends.get(friend_public_key).unwrap();

    // Send pending requests:
    // TODO: Possibly replace this clone with something more efficient later:
    let mut pending_requests = friend.pending_requests.clone();
    while let Some(pending_request) = pending_requests.pop_front() {
        let pending_op = FriendTcOp::RequestSendFunds(pending_request);
        queue_operation_or_cancel(
            m_state,
            pending_move_token,
            cancel_public_keys,
            outgoing_control,
            &pending_op,
        )?;
        let friend_mutation = FriendMutation::PopFrontPendingRequest;
        let funder_mutation =
            FunderMutation::FriendMutation((friend_public_key.clone(), friend_mutation));
        m_state.mutate(funder_mutation);
    }

    let friend = m_state.state().friends.get(friend_public_key).unwrap();

    // Send as many pending user requests as possible:
    let mut pending_user_requests = friend.pending_user_requests.clone();
    while let Some(request_send_funds) = pending_user_requests.pop_front() {
        let pending_op = FriendTcOp::RequestSendFunds(request_send_funds);
        queue_operation_or_cancel(
            m_state,
            pending_move_token,
            cancel_public_keys,
            outgoing_control,
            &pending_op,
        )?;
        let friend_mutation = FriendMutation::PopFrontPendingUserRequest;
        let funder_mutation =
            FunderMutation::FriendMutation((friend_public_key.clone(), friend_mutation));
        m_state.mutate(funder_mutation);
    }
    Ok(())
}

fn append_cancels_to_move_token<B>(
    m_state: &mut MutableFunderState<B>,
    friend_public_key: &PublicKey,
    pending_move_token: &mut PendingMoveToken<B>,
) -> Result<(), CollectOutgoingError>
where
    B: Clone + CanonicalSerialize + PartialEq + Eq + Debug,
{
    let friend = m_state.state().friends.get(friend_public_key).unwrap();

    // Send pending responses (Response, Cancel, Collect):
    // TODO: Possibly replace this clone with something more efficient later:
    let mut pending_backwards_ops = friend.pending_backwards_ops.clone();
    while let Some(pending_backwards_op) = pending_backwards_ops.pop_front() {
        let pending_op = backwards_op_to_friend_tc_op(pending_backwards_op);
        // TODO: Find a more elegant way to do this:
        let mut dummy_cancel_public_keys = HashSet::new();
        let mut dummy_outgoing_control = Vec::new();
        queue_operation_or_cancel(
            m_state,
            pending_move_token,
            &mut dummy_cancel_public_keys,
            &mut dummy_outgoing_control,
            &pending_op,
        )?;

        let friend_mutation = FriendMutation::PopFrontPendingBackwardsOp;
        let funder_mutation =
            FunderMutation::FriendMutation((friend_public_key.clone(), friend_mutation));
        m_state.mutate(funder_mutation);
    }
    Ok(())
}

async fn send_move_token<'a, B, R>(
    m_state: &'a mut MutableFunderState<B>,
    friend_public_key: PublicKey,
    pending_move_token: PendingMoveToken<B>,
    identity_client: &'a mut IdentityClient,
    rng: &'a R,
    outgoing_messages: &'a mut Vec<OutgoingMessage<B>>,
) where
    B: Clone + CanonicalSerialize + PartialEq + Eq + Debug,
    R: CryptoRandom,
{
    let PendingMoveToken {
        operations,
        opt_local_relays,
        token_wanted,
        may_send_empty,
        ..
    } = pending_move_token;

    if operations.is_empty() && opt_local_relays.is_none() && !may_send_empty {
        return;
    }

    // We want the token back if we just set a new address, to be sure
    // that the remote side knows about the new address.
    let token_wanted = token_wanted || opt_local_relays.is_some();

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

    let u_move_token =
        tc_incoming.create_unsigned_move_token(operations, opt_local_relays, rand_nonce);

    let move_token = await!(sign_move_token(u_move_token, identity_client));

    let tc_mutation = TcMutation::SetDirection(SetDirection::Outgoing(move_token));
    let friend_mutation = FriendMutation::TcMutation(tc_mutation);
    let funder_mutation =
        FunderMutation::FriendMutation((friend_public_key.clone(), friend_mutation));
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

    outgoing_messages.push((
        friend_public_key.clone(),
        FriendMessage::MoveTokenRequest(move_token_request),
    ));
}

fn init_cancel_pending_move_token<B>(
    m_state: &mut MutableFunderState<B>,
    ephemeral: &Ephemeral,
    max_operations_in_batch: usize,
    cancel_public_keys: &HashSet<PublicKey>,
    pending_move_tokens: &mut HashMap<PublicKey, PendingMoveToken<B>>,
) where
    B: Clone + Eq + CanonicalSerialize + Debug,
{
    let pending_move_token_keys = pending_move_tokens.keys().cloned().collect::<HashSet<_>>();
    for friend_public_key in cancel_public_keys {
        // Make sure that this friend is ready,
        // and that it doesn't already have a PendingMoveToken:

        if !ephemeral.liveness.is_online(&friend_public_key)
            || pending_move_token_keys.contains(&friend_public_key)
        {
            continue;
        }

        let friend = m_state.state().friends.get(friend_public_key).unwrap();

        // We expect that this friend has a consistent channel,
        // because we just attempted to forward a request that originated from
        // this friend.
        let token_channel = match &friend.channel_status {
            ChannelStatus::Consistent(token_channel) => token_channel,
            ChannelStatus::Inconsistent(_) => unreachable!(),
        };
        let tc_incoming = match &token_channel.get_direction() {
            TcDirection::Outgoing(_) => continue,
            TcDirection::Incoming(tc_incoming) => tc_incoming,
        };
        let outgoing_mc = tc_incoming.begin_outgoing_move_token();

        let may_send_empty = false;
        let pending_move_token = PendingMoveToken::new(
            friend_public_key.clone(),
            outgoing_mc,
            max_operations_in_batch,
            may_send_empty,
        );
        pending_move_tokens.insert(friend_public_key.clone(), pending_move_token);
    }
}

/// Send all possible messages according to SendCommands
pub async fn create_friend_messages<'a, B, R>(
    m_state: &'a mut MutableFunderState<B>,
    ephemeral: &'a Ephemeral,
    send_commands: &'a SendCommands,
    max_operations_in_batch: usize,
    identity_client: &'a mut IdentityClient,
    rng: &'a R,
) -> (
    Vec<FunderOutgoingControl<B>>,
    Vec<OutgoingMessage<B>>,
    Vec<ChannelerConfig<RelayAddress<B>>>,
)
where
    B: Clone + PartialEq + Eq + CanonicalSerialize + Debug,
    R: CryptoRandom,
{
    let mut outgoing_control = Vec::new();
    let mut outgoing_messages = Vec::new();
    let mut outgoing_channeler_config = Vec::new();
    let mut pending_move_tokens: HashMap<PublicKey, PendingMoveToken<B>> = HashMap::new();

    // First iteration:
    let mut cancel_public_keys = HashSet::new();
    for (friend_public_key, friend_send_commands) in &send_commands.send_commands {
        if !ephemeral.liveness.is_online(friend_public_key) {
            continue;
        }
        await!(send_friend_iter1(
            m_state,
            friend_public_key,
            friend_send_commands,
            &mut pending_move_tokens,
            identity_client,
            rng,
            max_operations_in_batch,
            &mut cancel_public_keys,
            &mut outgoing_messages,
            &mut outgoing_control,
            &mut outgoing_channeler_config
        ));
    }

    // Create PendingMoveToken-s for all the friends that were queued
    // new pending messages during `send_friend_iter1`:
    init_cancel_pending_move_token(
        m_state,
        ephemeral,
        max_operations_in_batch,
        &cancel_public_keys,
        &mut pending_move_tokens,
    );

    // Second iteration (Attempt to queue Cancel-s created in the first iteration):
    for (friend_public_key, pending_move_token) in &mut pending_move_tokens {
        assert!(ephemeral.liveness.is_online(&friend_public_key));
        let _ = append_cancels_to_move_token(m_state, friend_public_key, pending_move_token);
    }

    // Send all pending move tokens:
    for (friend_public_key, pending_move_token) in pending_move_tokens.into_iter() {
        assert!(ephemeral.liveness.is_online(&friend_public_key));
        await!(send_move_token(
            m_state,
            friend_public_key,
            pending_move_token,
            identity_client,
            rng,
            &mut outgoing_messages
        ));
    }

    (
        outgoing_control,
        outgoing_messages,
        outgoing_channeler_config,
    )
}
