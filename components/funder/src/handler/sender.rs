use std::fmt::Debug;
use crypto::identity::PublicKey;
use crypto::crypto_rand::{RandValue, CryptoRandom};

use proto::funder::messages::{FriendTcOp, FriendMessage, RequestsStatus};
use common::canonical_serialize::CanonicalSerialize;

use super::MutableFunderHandler;


use crate::state::{FunderMutation};
use crate::types::{FunderOutgoingComm, create_pending_request};
use crate::mutual_credit::outgoing::{QueueOperationFailure, QueueOperationError, OutgoingMc};
use crate::mutual_credit::types::McMutation;

use crate::friend::{FriendMutation, ResponseOp, ChannelStatus, SentLocalAddress};
use crate::token_channel::{TcMutation, TcDirection, SetDirection};

use crate::ephemeral::EphemeralMutation;
use crate::freeze_guard::FreezeGuardMutation;

pub enum SendMode {
    EmptyAllowed,
    EmptyNotAllowed,
}



impl<A,R> MutableFunderHandler<A,R> 
where
    A: CanonicalSerialize + Clone + Debug + PartialEq + Eq + 'static,
    R: CryptoRandom,
{
    /// Queue as many messages as possible into available token channel.
    fn queue_outgoing_operations(&mut self,
                           remote_public_key: &PublicKey,
                           outgoing_mc: &mut OutgoingMc)
                -> Result<(), QueueOperationFailure> {


        let friend = self.get_friend(remote_public_key).unwrap();

        // Set remote_max_debt if needed:
        let remote_max_debt = match &friend.channel_status {
            ChannelStatus::Consistent(token_channel) => token_channel,
            ChannelStatus::Inconsistent(_) => unreachable!(),
        }.get_remote_max_debt();


        if friend.wanted_remote_max_debt != remote_max_debt {
            outgoing_mc.queue_operation(FriendTcOp::SetRemoteMaxDebt(friend.wanted_remote_max_debt))?;
        }

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
            outgoing_mc.queue_operation(friend_op)?;
        }

        // Send pending responses (responses and failures)
        // TODO: Possibly replace this clone with something more efficient later:
        let mut pending_responses = friend.pending_responses.clone();
        while let Some(pending_response) = pending_responses.pop_front() {
            let pending_op = match pending_response {
                ResponseOp::Response(response) => FriendTcOp::ResponseSendFunds(response),
                ResponseOp::Failure(failure) => FriendTcOp::FailureSendFunds(failure),
            };
            outgoing_mc.queue_operation(pending_op)?;
            let friend_mutation = FriendMutation::PopFrontPendingResponse;
            let funder_mutation = FunderMutation::FriendMutation((remote_public_key.clone(), friend_mutation));
            self.apply_funder_mutation(funder_mutation);
        }

        let friend = self.get_friend(remote_public_key).unwrap();

        // Send pending requests:
        // TODO: Possibly replace this clone with something more efficient later:
        let mut pending_requests = friend.pending_requests.clone();
        while let Some(pending_request) = pending_requests.pop_front() {
            let pending_op = FriendTcOp::RequestSendFunds(pending_request);
            outgoing_mc.queue_operation(pending_op)?;
            let friend_mutation = FriendMutation::PopFrontPendingRequest;
            let funder_mutation = FunderMutation::FriendMutation((remote_public_key.clone(), friend_mutation));
            self.apply_funder_mutation(funder_mutation);
        }

        let friend = self.get_friend(remote_public_key).unwrap();

        // Send as many pending user requests as possible:
        let mut pending_user_requests = friend.pending_user_requests.clone();
        while let Some(request_send_funds) = pending_user_requests.pop_front() {
            let request_op = FriendTcOp::RequestSendFunds(request_send_funds);
            outgoing_mc.queue_operation(request_op)?;
            let friend_mutation = FriendMutation::PopFrontPendingUserRequest;
            let funder_mutation = FunderMutation::FriendMutation((remote_public_key.clone(), friend_mutation));
            self.apply_funder_mutation(funder_mutation);
        }
        Ok(())
    }

    /// Transmit the current outgoing friend_move_token.
    pub fn transmit_outgoing(&mut self,
                               remote_public_key: &PublicKey) {

        let friend = self.get_friend(remote_public_key).unwrap();
        let token_channel = match &friend.channel_status {
            ChannelStatus::Consistent(token_channel) => token_channel,
            ChannelStatus::Inconsistent(_) => unreachable!(),
        };

        let friend_move_token_request = match &token_channel.get_direction() {
            TcDirection::Outgoing(tc_outgoing) => tc_outgoing.create_outgoing_move_token_request(),
            TcDirection::Incoming(_) => unreachable!(),
        };

        // Transmit the current outgoing message:
        self.add_outgoing_comm(FunderOutgoingComm::FriendMessage(
            (remote_public_key.clone(),
                FriendMessage::MoveTokenRequest(friend_move_token_request))));
    }

    async fn send_friend_move_token<'a>(&'a mut self,
                           remote_public_key: &'a PublicKey,
                           operations: Vec<FriendTcOp>,
                           opt_local_address: Option<A>,
                           mc_mutations: Vec<McMutation>) {

        if let Some(local_address) = &opt_local_address {
            let friend = self.get_friend(remote_public_key).unwrap();

            let sent_local_address = match &friend.sent_local_address {
                SentLocalAddress::NeverSent => SentLocalAddress::LastSent(local_address.clone()),
                SentLocalAddress::Transition((_last_address, _prev_last_address)) => 
                    // We have the token, this means that there couldn't be a transition right now.
                    unreachable!(),
                SentLocalAddress::LastSent(last_address) =>
                    SentLocalAddress::Transition((local_address.clone(), last_address.clone())),
            };

            let friend_mutation = FriendMutation::SetSentLocalAddress(sent_local_address);
            let funder_mutation = FunderMutation::FriendMutation((remote_public_key.clone(), friend_mutation));
            self.apply_funder_mutation(funder_mutation);
        }

        for mc_mutation in mc_mutations {
            let tc_mutation = TcMutation::McMutation(mc_mutation);
            let friend_mutation = FriendMutation::TcMutation(tc_mutation);
            let funder_mutation = FunderMutation::FriendMutation((remote_public_key.clone(), friend_mutation));
            self.apply_funder_mutation(funder_mutation);
        }

        // Update freeze guard about outgoing requests:
        for operation in &operations {
            if let FriendTcOp::RequestSendFunds(request_send_funds) = operation {
                let pending_request = &create_pending_request(&request_send_funds);

                let freeze_guard_mutation = FreezeGuardMutation::AddFrozenCredit(
                    (pending_request.route.clone(), pending_request.dest_payment));
                let ephemeral_mutation = EphemeralMutation::FreezeGuardMutation(freeze_guard_mutation);
                self.apply_ephemeral_mutation(ephemeral_mutation);
            }
        }

        let friend = self.get_friend(remote_public_key).unwrap();

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
        let funder_mutation = FunderMutation::FriendMutation((remote_public_key.clone(), friend_mutation));
        self.apply_funder_mutation(funder_mutation);

        let friend = self.get_friend(remote_public_key).unwrap();
        let token_channel = match &friend.channel_status {
            ChannelStatus::Consistent(token_channel) => token_channel,
            ChannelStatus::Inconsistent(_) => unreachable!(),
        };

        let tc_outgoing = match token_channel.get_direction() {
            TcDirection::Outgoing(tc_outgoing) => tc_outgoing,
            TcDirection::Incoming(_) => unreachable!(),
        };

        let friend_move_token_request = tc_outgoing.create_outgoing_move_token_request();

        // Add a task for sending the outgoing move token:
        self.add_outgoing_comm(FunderOutgoingComm::FriendMessage(
            (remote_public_key.clone(),
                FriendMessage::MoveTokenRequest(friend_move_token_request))));
    }

    /// Compose a large as possible message to send through the token channel to the remote side.
    /// The message should contain various operations, collected from:
    /// - Generic pending requests (Might be sent through any token channel).
    /// - Token channel specific pending responses/failures.
    /// - Commands that were initialized through AppManager.
    ///
    /// Any operations that will enter the message should be applied. For example, a failure
    /// message should cause the pending request to be removed.
    ///
    /// Returns whether a move token message is scheduled for the remote side.
    async fn send_through_token_channel<'a>(&'a mut self, 
                                  remote_public_key: &'a PublicKey,
                                  send_mode: SendMode) -> bool {

        let friend = self.get_friend(remote_public_key).unwrap();
        let token_channel = match &friend.channel_status {
            ChannelStatus::Consistent(token_channel) => token_channel,
            ChannelStatus::Inconsistent(_) => unreachable!(),
        };
        let tc_incoming = match token_channel.get_direction() {
            TcDirection::Outgoing(_) => unreachable!(),
            TcDirection::Incoming(tc_incoming) => tc_incoming,
        };

        let mut outgoing_mc = tc_incoming.begin_outgoing_move_token();
        if let Err(queue_operation_failure) = 
                self.queue_outgoing_operations(remote_public_key, &mut outgoing_mc) {
            if let QueueOperationError::MaxOperationsReached = queue_operation_failure.error { 
            } else {
                unreachable!();
            }
        }
        let (operations, mc_mutations) = outgoing_mc.done();

        // Check if notification about local address change is required:
        let opt_local_address = match &self.state.opt_address {
            Some(local_address) => {
                let friend = self.get_friend(remote_public_key).unwrap();
                match &friend.sent_local_address {
                    SentLocalAddress::NeverSent => Some(local_address.clone()),
                    SentLocalAddress::Transition((_last_address, _prev_last_address)) => unreachable!(),
                    SentLocalAddress::LastSent(last_address) => {
                        if last_address != local_address {
                            Some(local_address.clone())
                        } else {
                            None
                        }
                    }
                }
            },
            None => None,
        };

        let may_send_empty = if let SendMode::EmptyAllowed = send_mode {true} else {false};
        if may_send_empty || !operations.is_empty() || opt_local_address.is_some() {
            await!(self.send_friend_move_token(remote_public_key, 
                                               operations, 
                                               opt_local_address,
                                               mc_mutations));
            true
        } else {
            false
        }
    }

    /// Try to send whatever possible through a friend channel.
    pub async fn try_send_channel<'a>(&'a mut self,
                        remote_public_key: &'a PublicKey,
                        send_mode: SendMode) {

        let friend = self.get_friend(remote_public_key).unwrap();

        // We do not send messages if we are in an inconsistent status:
        let token_channel = match &friend.channel_status {
            ChannelStatus::Consistent(token_channel) => token_channel,
            ChannelStatus::Inconsistent(_) => return,
        };

        match &token_channel.get_direction() {
            TcDirection::Incoming(_) => {
                // We have the token. 
                // Send as many operations as possible to remote side:
                await!(self.send_through_token_channel(&remote_public_key, send_mode));
            },
            TcDirection::Outgoing(tc_outgoing) => {
                if !tc_outgoing.token_wanted {
                    // We don't have the token. We should request it.
                    // Mark that we have sent a request token, to make sure we don't do this again:
                    let tc_mutation = TcMutation::SetTokenWanted;
                    let friend_mutation = FriendMutation::TcMutation(tc_mutation);
                    let funder_mutation = FunderMutation::FriendMutation((remote_public_key.clone(), friend_mutation));
                    self.apply_funder_mutation(funder_mutation);
                    self.transmit_outgoing(remote_public_key);
                }
            },
        };
    }
}
