use std::fmt::Debug;
use std::collections::HashMap;

use crypto::identity::PublicKey;
use crypto::crypto_rand::{RandValue, CryptoRandom};

use proto::funder::messages::{FriendTcOp, FriendMessage, RequestsStatus, MoveTokenRequest};
use common::canonical_serialize::CanonicalSerialize;

use super::MutableFunderHandler;

use crate::types::{FunderOutgoingComm, create_pending_request};
use crate::mutual_credit::outgoing::{QueueOperationError, OutgoingMc};

use crate::friend::{FriendMutation, ResponseOp, 
    ChannelStatus, SentLocalAddress};
use crate::token_channel::{TcMutation, TcDirection, SetDirection};

use crate::freeze_guard::FreezeGuardMutation;

use crate::state::{FunderMutation};
use crate::ephemeral::{EphemeralMutation};


#[derive(Debug, Clone)]
pub struct FriendSendCommands {
    /// Try to send whatever possible through this friend.
    pub try_send: bool,
    /// Resend the outgoing move token message
    pub resend_outgoing: bool,
    /// Remote friend wants the token.
    pub wants_token: bool,
}

impl FriendSendCommands {
    fn new() -> Self {
        FriendSendCommands {
            try_send: false,
            resend_outgoing: false,
            wants_token: false,
        }
    }
}

/*
pub enum SendMode {
    EmptyAllowed,
    EmptyNotAllowed,
}
*/

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

pub struct PendingMoveToken<A> {
    pub outgoing_mc: OutgoingMc,
    pub operations: Vec<FriendTcOp>,
    pub opt_local_address: Option<A>,
    pub token_wanted: bool,
}

impl<A> PendingMoveToken<A> {
    fn new(outgoing_mc: OutgoingMc) -> Self {
        PendingMoveToken {
            outgoing_mc,
            operations: Vec::new(),
            opt_local_address: None,
            token_wanted: false,
        }
    }
}


impl<A,R> MutableFunderHandler<A,R> 
where
    A: CanonicalSerialize + Clone + Debug + PartialEq + Eq + 'static,
    R: CryptoRandom + 'static,
{
    pub fn set_try_send(&mut self, friend_public_key: &PublicKey) {
        let friend_send_commands = self.send_commands
            .entry(friend_public_key.clone())
            .or_insert(FriendSendCommands::new());
        friend_send_commands.try_send = true;
    }

    pub fn set_resend_outgoing(&mut self, friend_public_key: &PublicKey) {
        let friend_send_commands = self.send_commands
            .entry(friend_public_key.clone())
            .or_insert(FriendSendCommands::new());
        friend_send_commands.resend_outgoing = true;
    }

    pub fn set_wants_token(&mut self, friend_public_key: &PublicKey) {
        let friend_send_commands = self.send_commands
            .entry(friend_public_key.clone())
            .or_insert(FriendSendCommands::new());
        friend_send_commands.wants_token = true;
    }

    /// Transmit the current outgoing friend_move_token.
    pub fn transmit_outgoing(&mut self,
                               remote_public_key: &PublicKey,
                               token_wanted: bool) {

        let friend = self.get_friend(remote_public_key).unwrap();
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

        // Transmit the current outgoing message:
        self.add_outgoing_comm(FunderOutgoingComm::FriendMessage(
            (remote_public_key.clone(),
                FriendMessage::MoveTokenRequest(move_token_request))));
    }


    /// Do we need to send anything to the remote side?
    /// Note that this is only an estimation. It is possible that when the token from remote side
    /// arrives, the state will be different.
    pub fn estimate_should_send(&self, 
                                 friend_public_key: &PublicKey) -> bool {

        let friend = self.get_friend(friend_public_key).unwrap();

        // Check if notification about local address change is required:
        if let Some(local_address) = &self.state.opt_address {
            let friend = self.get_friend(friend_public_key).unwrap();
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

    /// Attempt to queue one operation into a certain `pending_move_token`.
    /// If successful, mutations are applied and the operation is queued.
    /// Otherwise, an error is returned.
    fn queue_operation(&mut self, 
                       friend_public_key: &PublicKey,
                       pending_move_token: &mut PendingMoveToken<A>,
                       operation: &FriendTcOp)
        -> Result<(), PendingQueueError> {

        if pending_move_token.operations.len() >= self.max_operations_in_batch {
            return Err(PendingQueueError::MaxOperationsReached);
        }

        // Freeze guard check (Only for requests):
        if let FriendTcOp::RequestSendFunds(request_send_funds) = operation {
            let verify_res = self.ephemeral
                .freeze_guard
                .verify_freezing_links(&request_send_funds.route,
                                        request_send_funds.dest_payment,
                                       &request_send_funds.freeze_links);
            if verify_res.is_none() {
                return Err(PendingQueueError::FreezeGuardBlock);
            }
        }

        let mc_mutations = match pending_move_token.outgoing_mc.queue_operation(operation) {
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
            self.apply_ephemeral_mutation(ephemeral_mutation);
        }

        // Apply mutations:
        for mc_mutation in mc_mutations {
            let tc_mutation = TcMutation::McMutation(mc_mutation);
            let friend_mutation = FriendMutation::TcMutation(tc_mutation);
            let funder_mutation = FunderMutation::FriendMutation((friend_public_key.clone(), friend_mutation));
            self.apply_funder_mutation(funder_mutation);
        }

        Ok(())
    }

    /// Set local address inside pending move token.
    fn set_local_address(&self, 
                         pending_move_token: &mut PendingMoveToken<A>,
                         local_address: A) {

        pending_move_token.opt_local_address = Some(local_address);
    }

    pub async fn queue_operation_or_failure<'a>(&'a mut self, friend_public_key: &'a PublicKey,
                                            pending_move_token: &'a mut PendingMoveToken<A>,
                                            operation: &'a FriendTcOp) -> Result<(), CollectOutgoingError> {

        let res = self.queue_operation(friend_public_key,
                             pending_move_token,
                             operation);
        match res {
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
        let pending_request = create_pending_request(&request_send_funds);
        let failure_send_funds = await!(self.create_failure_message(pending_request));

        let failure_op = ResponseOp::Failure(failure_send_funds);
        let friend_mutation = FriendMutation::PushBackPendingResponse(failure_op);
        let funder_mutation = FunderMutation::FriendMutation((friend_public_key.clone(), friend_mutation));
        self.apply_funder_mutation(funder_mutation);

        Ok(())
    }

    /// Given a friend with an incoming move token state, create the largest possible move token to
    /// send to the remote side. 
    /// Requests that fail to be processed are moved to the failure queues of the relevant friends.
    pub async fn collect_outgoing_move_token<'a>(&'a mut self, friend_public_key: &'a PublicKey,
                                   pending_move_token: &'a mut PendingMoveToken<A>) 
        -> Result<(), CollectOutgoingError> {

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

        let friend = self.get_friend(friend_public_key).unwrap();

        // Set remote_max_debt if needed:
        let remote_max_debt = match &friend.channel_status {
            ChannelStatus::Consistent(token_channel) => token_channel,
            ChannelStatus::Inconsistent(_) => unreachable!(),
        }.get_remote_max_debt();


        if friend.wanted_remote_max_debt != remote_max_debt {
            let operation = FriendTcOp::SetRemoteMaxDebt(friend.wanted_remote_max_debt);
            await!(self.queue_operation_or_failure(friend_public_key,
                                 pending_move_token,
                                 &operation))?;
        }

        let friend = self.get_friend(friend_public_key).unwrap();
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
            await!(self.queue_operation_or_failure(friend_public_key,
                                 pending_move_token,
                                 &friend_op))?;
        }

        let friend = self.get_friend(friend_public_key).unwrap();
        // Send pending responses (responses and failures)
        // TODO: Possibly replace this clone with something more efficient later:
        let mut pending_responses = friend.pending_responses.clone();
        while let Some(pending_response) = pending_responses.pop_front() {
            let pending_op = match pending_response {
                ResponseOp::Response(response) => FriendTcOp::ResponseSendFunds(response),
                ResponseOp::Failure(failure) => FriendTcOp::FailureSendFunds(failure),
            };
            await!(self.queue_operation_or_failure(friend_public_key,
                                 pending_move_token,
                                 &pending_op))?;

            let friend_mutation = FriendMutation::PopFrontPendingResponse;
            let funder_mutation = FunderMutation::FriendMutation((friend_public_key.clone(), friend_mutation));
            self.apply_funder_mutation(funder_mutation);
        }

        let friend = self.get_friend(friend_public_key).unwrap();

        // Send pending requests:
        // TODO: Possibly replace this clone with something more efficient later:
        let mut pending_requests = friend.pending_requests.clone();
        while let Some(pending_request) = pending_requests.pop_front() {
            let pending_op = FriendTcOp::RequestSendFunds(pending_request);
            await!(self.queue_operation_or_failure(friend_public_key,
                                 pending_move_token,
                                 &pending_op))?;
            let friend_mutation = FriendMutation::PopFrontPendingRequest;
            let funder_mutation = FunderMutation::FriendMutation((friend_public_key.clone(), friend_mutation));
            self.apply_funder_mutation(funder_mutation);
        }

        let friend = self.get_friend(friend_public_key).unwrap();

        // Send as many pending user requests as possible:
        let mut pending_user_requests = friend.pending_user_requests.clone();
        while let Some(request_send_funds) = pending_user_requests.pop_front() {
            let request_op = FriendTcOp::RequestSendFunds(request_send_funds);
            await!(self.queue_operation_or_failure(friend_public_key,
                                 pending_move_token,
                                 &request_op))?;
            let friend_mutation = FriendMutation::PopFrontPendingUserRequest;
            let funder_mutation = FunderMutation::FriendMutation((friend_public_key.clone(), friend_mutation));
            self.apply_funder_mutation(funder_mutation);
        }
        Ok(())
    }

    pub async fn send_friend_iter1<'a>(&'a mut self, friend_public_key: &'a PublicKey, 
                             friend_send_commands: &'a FriendSendCommands, 
                             pending_move_tokens: &'a mut HashMap<PublicKey, PendingMoveToken<A>>) {

        if !friend_send_commands.try_send 
            && !friend_send_commands.resend_outgoing 
            && !friend_send_commands.wants_token {

            return;
        }

        let friend = match self.get_friend(&friend_public_key) {
            None => return,
            Some(friend) => friend,
        };

        let token_channel = match &friend.channel_status {
            ChannelStatus::Consistent(token_channel) => token_channel,
            ChannelStatus::Inconsistent(channel_inconsistent) => {
                if friend_send_commands.resend_outgoing {
                    self.add_outgoing_comm(FunderOutgoingComm::FriendMessage((friend_public_key.clone(),
                            FriendMessage::InconsistencyError(channel_inconsistent.local_reset_terms.clone()))));
                }
                return;
            },
        };

        let tc_incoming = match token_channel.get_direction() {
            TcDirection::Outgoing(_) => {
                if self.estimate_should_send(friend_public_key) {
                    let is_token_wanted = true;
                    self.transmit_outgoing(&friend_public_key, is_token_wanted);
                } else {
                    if friend_send_commands.resend_outgoing {
                        let is_token_wanted = false;
                        self.transmit_outgoing(&friend_public_key, is_token_wanted);
                    }
                }
                return;
            },
            TcDirection::Incoming(tc_incoming) => tc_incoming,
        };

        // If we are here, the token channel is incoming:

        // It will be strange if we need to resend outgoing, because the channel
        // is in incoming mode.
        assert!(!friend_send_commands.resend_outgoing);

        let outgoing_mc = tc_incoming.begin_outgoing_move_token();
        let pending_move_token = PendingMoveToken::new(outgoing_mc);
        pending_move_tokens.insert(friend_public_key.clone(), pending_move_token);
        let pending_move_token = pending_move_tokens.get_mut(friend_public_key).unwrap();
        let _ = await!(self.collect_outgoing_move_token(friend_public_key, pending_move_token));
    }

    pub async fn append_failures_to_move_token<'a>(&'a mut self, friend_public_key: &'a PublicKey,
                                   pending_move_token: &'a mut PendingMoveToken<A>) 
        -> Result<(), CollectOutgoingError> {

        let friend = self.get_friend(friend_public_key).unwrap();

        // Send pending responses (responses and failures)
        // TODO: Possibly replace this clone with something more efficient later:
        let mut pending_responses = friend.pending_responses.clone();
        while let Some(pending_response) = pending_responses.pop_front() {
            let pending_op = match pending_response {
                ResponseOp::Response(response) => FriendTcOp::ResponseSendFunds(response),
                ResponseOp::Failure(failure) => FriendTcOp::FailureSendFunds(failure),
            };
            await!(self.queue_operation_or_failure(friend_public_key,
                                 pending_move_token,
                                 &pending_op))?;

            let friend_mutation = FriendMutation::PopFrontPendingResponse;
            let funder_mutation = FunderMutation::FriendMutation((friend_public_key.clone(), friend_mutation));
            self.apply_funder_mutation(funder_mutation);
        }
        Ok(())
    }

    pub async fn send_move_token<'a>(&'a mut self,
                                      friend_public_key: PublicKey,
                                      pending_move_token: PendingMoveToken<A>) {
        let PendingMoveToken {
            outgoing_mc: _outgoing_mc,
            operations,
            opt_local_address,
            token_wanted,
        } = pending_move_token;

        let friend = self.get_friend(&friend_public_key).unwrap();

        let rand_nonce = RandValue::new(&self.rng);
        let token_channel = match &friend.channel_status {
            ChannelStatus::Consistent(token_channel) => token_channel,
            ChannelStatus::Inconsistent(_) => unreachable!(),
        };

        let tc_incoming = match token_channel.get_direction() {
            TcDirection::Outgoing(_) => unreachable!(),
            TcDirection::Incoming(tc_incoming) => tc_incoming,
        };

        let friend_move_token = await!(tc_incoming.create_friend_move_token(operations, 
                                             opt_local_address,
                                             rand_nonce,
                                             self.identity_client.clone()));

        let tc_mutation = TcMutation::SetDirection(
            SetDirection::Outgoing(friend_move_token));
        let friend_mutation = FriendMutation::TcMutation(tc_mutation);
        let funder_mutation = FunderMutation::FriendMutation((friend_public_key.clone(), friend_mutation));
        self.apply_funder_mutation(funder_mutation);

        let friend = self.get_friend(&friend_public_key).unwrap();
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

        // Add a task for sending the outgoing move token:
        self.add_outgoing_comm(FunderOutgoingComm::FriendMessage(
            (friend_public_key.clone(),
                FriendMessage::MoveTokenRequest(move_token_request))));

    }

    /// Send all possible messages according to SendCommands
    pub async fn send(&mut self) {
        let mut pending_move_tokens: HashMap<PublicKey, PendingMoveToken<A>> 
            = HashMap::new();

        let send_commands = self.send_commands.clone();
        // First iteration:
        for (friend_public_key, friend_send_commands) in &send_commands {
            await!(self.send_friend_iter1(&friend_public_key,
                                          &friend_send_commands,
                                          &mut pending_move_tokens));
        }

        // Second iteration (Attempt to queue failures created in the first iteration):
        for (friend_public_key, pending_move_token) in &mut pending_move_tokens {
            let _ = await!(self.append_failures_to_move_token(friend_public_key, pending_move_token));
        }

        // Send all pending move tokens:
        for (friend_public_key, pending_move_token) in pending_move_tokens.into_iter() {
            await!(self.send_move_token(friend_public_key, pending_move_token));
        }
    }
}



