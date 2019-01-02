use std::fmt::Debug;
use byteorder::{BigEndian, WriteBytesExt};

use crypto::crypto_rand::{RandValue, CryptoRandom};
use crypto::identity::{PublicKey, Signature};
use crypto::hash::sha_512_256;

use identity::IdentityClient;

use common::canonical_serialize::CanonicalSerialize;

use proto::funder::messages::{RequestSendFunds, ResponseSendFunds,
    FailureSendFunds, MoveToken, FreezeLink, FriendMessage,
    MoveTokenRequest, ResetTerms, PendingRequest, ResponseReceived,
    FunderOutgoingControl, ResponseSendFundsResult};
use proto::funder::signature_buff::{create_response_signature_buffer, prepare_receipt};

use crate::mutual_credit::incoming::{IncomingResponseSendFunds, 
    IncomingFailureSendFunds, IncomingMessage};
use crate::token_channel::{ReceiveMoveTokenOutput, ReceiveMoveTokenError, 
    MoveTokenReceived, TokenChannel};

use crate::types::{FunderOutgoingComm, create_pending_request};

use crate::state::FunderMutation;
use crate::friend::{FriendMutation, 
    ResponseOp, ChannelStatus, ChannelInconsistent,
    SentLocalAddress};


use crate::ephemeral::EphemeralMutation;
use crate::freeze_guard::FreezeGuardMutation;

use super::{MutableFunderHandler};
use super::sender::SendMode;


#[derive(Debug)]
pub enum HandleFriendError {
    FriendDoesNotExist,
    NoMoveTokenToAck,
    AlreadyAcked,
    TokenNotOwned,
    IncorrectAckedToken,
    IncorrectLastToken,
    TokenChannelInconsistent,
    NotInconsistent,
    ResetTokenMismatch,
    InconsistencyWhenTokenOwned,
}

// Prefix used for chain hashing of token channel funds.
// NEXT is used for hashing for the next move token funds.
// RESET is used for resetting the token channel.
// The prefix allows the receiver to distinguish between the two cases.
// const TOKEN_NEXT: &[u8] = b"NEXT";
const TOKEN_RESET: &[u8] = b"RESET";

/// Calculate the token to be used for resetting the channel.
#[allow(unused)]
pub async fn calc_channel_reset_token(new_token: &Signature,
                      balance_for_reset: i128,
                      identity_client: IdentityClient) -> Signature {

    let mut sig_buffer = Vec::new();
    sig_buffer.extend_from_slice(&sha_512_256(TOKEN_RESET));
    sig_buffer.extend_from_slice(&new_token);
    sig_buffer.write_i128::<BigEndian>(balance_for_reset).unwrap();
    await!(identity_client.request_signature(sig_buffer)).unwrap()
}

pub async fn get_reset_terms<A>(token_channel: &TokenChannel<A>, 
                             identity_client: IdentityClient) -> ResetTerms 
where
    A: CanonicalSerialize + Clone,
{
    // We add 2 for the new counter in case 
    // the remote side has already used the next counter.
    let reset_token = await!(calc_channel_reset_token(
                             token_channel.get_new_token(),
                             token_channel.get_mutual_credit().balance_for_reset(),
                             identity_client));
    ResetTerms {
        reset_token,
        // TODO: Should we do something other than wrapping_add(1)?
        // 2**64 inconsistencies are required for an overflow.
        inconsistency_counter: token_channel.get_inconsistency_counter().wrapping_add(1),
        balance_for_reset: token_channel.get_mutual_credit().balance_for_reset(),
    }
}


#[allow(unused)]
impl<A,R> MutableFunderHandler<A,R> 
where
    A: CanonicalSerialize + Clone + Debug + Eq + PartialEq + 'static,
    R: CryptoRandom + 'static,
{

    /// Check if channel reset is required (Remove side used the RESET token)
    /// If so, reset the channel.
    pub fn try_reset_channel(&mut self, 
                           friend_public_key: &PublicKey,
                           local_reset_terms: &ResetTerms,
                           friend_move_token: &MoveToken<A>) {

        // Check if incoming message is an attempt to reset channel.
        // We can know this by checking if old_token is a special value.
        if friend_move_token.old_token == local_reset_terms.reset_token {
            // This is a reset message. We reset the token channel:
            let friend_mutation = FriendMutation::RemoteReset(friend_move_token.clone());
            let funder_mutation = FunderMutation::FriendMutation((friend_public_key.clone(), friend_mutation));
            self.apply_funder_mutation(funder_mutation);
        }

    }


    pub fn add_local_freezing_link(&self, request_send_funds: &mut RequestSendFunds) {
        let index = request_send_funds.route.pk_to_index(&self.state.local_public_key)
            .unwrap();
        assert_eq!(request_send_funds.freeze_links.len(), index);

        let next_index = index.checked_add(1).unwrap();
        let next_pk = request_send_funds.route.index_to_pk(next_index).unwrap();

        let opt_prev_pk = match index.checked_sub(1) {
            Some(prev_index) =>
                Some(request_send_funds.route.index_to_pk(prev_index).unwrap()),
            None => None,
        };

        let funder_freeze_link = FreezeLink {
            shared_credits: self.state.friends.get(&next_pk).unwrap().get_shared_credits(),
            usable_ratio: self.state.get_usable_ratio(opt_prev_pk, next_pk),
        };

        // Add our freeze link
        request_send_funds.freeze_links.push(funder_freeze_link);

    }

    /// Forward a request message to the relevant friend and token channel.
    async fn forward_request(&mut self, mut request_send_funds: RequestSendFunds) {
        let index = request_send_funds.route.pk_to_index(&self.state.local_public_key)
            .unwrap();
        let next_index = index.checked_add(1).unwrap();
        let next_pk = request_send_funds.route.index_to_pk(next_index).unwrap();

        // Queue message to the relevant friend. Later this message will be queued to a specific
        // available token channel:
        let friend_mutation = FriendMutation::PushBackPendingRequest(request_send_funds.clone());
        let funder_mutation = FunderMutation::FriendMutation((next_pk.clone(), friend_mutation));
        self.apply_funder_mutation(funder_mutation);
        await!(self.try_send_channel(&next_pk, SendMode::EmptyNotAllowed));
    }

    /// Create a (signed) failure message for a given request_id.
    /// We are the reporting_public_key for this failure message.
    async fn create_response_message(&self, request_send_funds: RequestSendFunds) 
        -> Result<ResponseSendFunds, !> {

        let rand_nonce = RandValue::new(&self.rng);
        let local_public_key = self.state.local_public_key.clone();

        let mut response_send_funds = ResponseSendFunds {
            request_id: request_send_funds.request_id,
            rand_nonce,
            signature: Signature::zero(),
        };

        let response_signature_buffer = create_response_signature_buffer(&response_send_funds,
                        &create_pending_request(&request_send_funds));

        response_send_funds.signature = await!(self.identity_client.request_signature(response_signature_buffer))
            .unwrap();

        Ok(response_send_funds)
    }

    async fn handle_request_send_funds(&mut self, 
                               remote_public_key: PublicKey,
                               mut request_send_funds: RequestSendFunds) {

        // Find ourselves on the route. If we are not there, abort.
        let remote_index = request_send_funds.route.find_pk_pair(
            &remote_public_key, 
            &self.state.local_public_key).unwrap();

        let local_index = remote_index.checked_add(1).unwrap();
        let next_index = local_index.checked_add(1).unwrap();
        if next_index >= request_send_funds.route.len() {
            // We are the destination of this request. We return a response:
            let response_send_funds = await!(self.create_response_message(request_send_funds.clone()))
                .unwrap();
            let response_op = ResponseOp::Response(response_send_funds);
            let friend_mutation = FriendMutation::PushBackPendingResponse(response_op);
            let funder_mutation = FunderMutation::FriendMutation((remote_public_key.clone(), friend_mutation));
            self.apply_funder_mutation(funder_mutation);
            return;
        }


        // The node on the route has to be one of our friends:
        let next_public_key = request_send_funds.route.index_to_pk(next_index).unwrap();
        let friend_exists = self.state.friends.contains_key(next_public_key);

        // This friend must be considered online for us to forward the message.
        // If we forward the request to an offline friend, the request could be stuck for a long
        // time before a response arrives.
        let friend_ready = if friend_exists {
            self.is_friend_ready(&next_public_key)
        } else {
            false
        };

        if !friend_ready {
            await!(self.reply_with_failure(remote_public_key.clone(), 
                                           request_send_funds.clone()));
            return;
        } 
        // Add our freezing link:
        self.add_local_freezing_link(&mut request_send_funds);

        // Perform DoS protection check:
        let verify_res = self.ephemeral
            .freeze_guard
            .verify_freezing_links(&request_send_funds.route,
                                               request_send_funds.dest_payment,
                                               &request_send_funds.freeze_links);
        match verify_res {
            Some(()) => {
                // Add our freezing link, and queue message to the next node.
                await!(self.forward_request(request_send_funds));
            },
            None => {
                // Queue a failure message to this token channel:
                await!(self.reply_with_failure(remote_public_key, 
                                               request_send_funds));
            },
        };
    }


    async fn handle_response_send_funds<'a>(&'a mut self, 
                               remote_public_key: &'a PublicKey,
                               response_send_funds: ResponseSendFunds,
                               pending_request: PendingRequest) {

        match self.find_request_origin(&response_send_funds.request_id).cloned() {
            None => {
                // We are the origin of this request, and we got a response.
                // We provide a receipt to the user:
                let receipt = prepare_receipt(&response_send_funds,
                                              &pending_request);

                let response_send_funds_result = ResponseSendFundsResult::Success(receipt.clone());
                self.add_outgoing_control(FunderOutgoingControl::ResponseReceived(
                    ResponseReceived {
                        request_id: pending_request.request_id.clone(),
                        result: response_send_funds_result,
                    }
                ));
                // We make our own copy of the receipt, in case the user abruptly crashes.
                // In that case the user will be able to obtain the receipt again later.
                let funder_mutation = FunderMutation::AddReceipt((pending_request.request_id, receipt));
                self.apply_funder_mutation(funder_mutation);
            },
            Some(friend_public_key) => {
                // Queue this response message to another token channel:
                let response_op = ResponseOp::Response(response_send_funds);
                let friend_mutation = FriendMutation::PushBackPendingResponse(response_op);
                let funder_mutation = FunderMutation::FriendMutation((friend_public_key.clone(), friend_mutation));
                self.apply_funder_mutation(funder_mutation);
                await!(self.try_send_channel(&friend_public_key, SendMode::EmptyNotAllowed));
            },
        }
    }

    async fn handle_failure_send_funds<'a>(&'a mut self, 
                               remote_public_key: &'a PublicKey,
                               failure_send_funds: FailureSendFunds,
                               pending_request: PendingRequest) {

        match self.find_request_origin(&failure_send_funds.request_id).cloned() {
            None => {
                // We are the origin of this request, and we got a failure
                // We should pass it back to crypter.


                let response_send_funds_result = ResponseSendFundsResult::Failure(failure_send_funds.reporting_public_key);
                self.add_outgoing_control(FunderOutgoingControl::ResponseReceived(
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
                self.apply_funder_mutation(funder_mutation);
                await!(self.try_send_channel(&friend_public_key, SendMode::EmptyNotAllowed));
            },
        };
    }

    /// Process valid incoming operations from remote side.
    async fn handle_move_token_output(&mut self, 
                                remote_public_key: PublicKey,
                                incoming_messages: Vec<IncomingMessage>) {

        for incoming_message in incoming_messages {
            match incoming_message {
                IncomingMessage::Request(request_send_funds) => {
                    await!(self.handle_request_send_funds(remote_public_key.clone(),
                                                 request_send_funds));
                },
                IncomingMessage::Response(IncomingResponseSendFunds {
                                                pending_request, incoming_response}) => {

                    let freeze_guard_mutation = FreezeGuardMutation::SubFrozenCredit(
                        (pending_request.route.clone(), pending_request.dest_payment));
                    let ephemeral_mutation = EphemeralMutation::FreezeGuardMutation(freeze_guard_mutation);
                    self.apply_ephemeral_mutation(ephemeral_mutation);
                    await!(self.handle_response_send_funds(&remote_public_key, 
                                                  incoming_response, pending_request));
                },
                IncomingMessage::Failure(IncomingFailureSendFunds {
                                                pending_request, incoming_failure}) => {

                    let freeze_guard_mutation = FreezeGuardMutation::SubFrozenCredit(
                        (pending_request.route.clone(), pending_request.dest_payment));
                    let ephemeral_mutation = EphemeralMutation::FreezeGuardMutation(freeze_guard_mutation);
                    self.apply_ephemeral_mutation(ephemeral_mutation);
                    await!(self.handle_failure_send_funds(&remote_public_key, 
                                                 incoming_failure, pending_request));
                },
            }
        }
    }


    /// Handle an error with incoming move token.
    async fn handle_move_token_error(&mut self,
                               remote_public_key: PublicKey,
                               receive_move_token_error: ReceiveMoveTokenError) {

        let friend = self.get_friend(&remote_public_key).unwrap();
        let token_channel = match &friend.channel_status {
            ChannelStatus::Consistent(token_channel) => token_channel,
            ChannelStatus::Inconsistent(_) => unreachable!(),
        };
        let opt_last_incoming_move_token = token_channel.get_last_incoming_move_token_hashed().cloned();
        // Send an InconsistencyError message to remote side:
        let local_reset_terms = await!(get_reset_terms(&token_channel, self.identity_client.clone()));

        self.add_outgoing_comm(FunderOutgoingComm::FriendMessage((remote_public_key.clone(),
                FriendMessage::InconsistencyError(local_reset_terms.clone()))));



        // Cancel all internal pending requests inside token channel:
        await!(self.cancel_local_pending_requests(
            remote_public_key.clone()));
        // Cancel all pending requests to this friend:
        await!(self.cancel_pending_requests(
                remote_public_key.clone()));
        await!(self.cancel_pending_user_requests(
                remote_public_key.clone()));

        // Keep outgoing InconsistencyError message details in memory:
        let channel_inconsistent = ChannelInconsistent {
            opt_last_incoming_move_token,
            local_reset_terms,
            opt_remote_reset_terms: None,
        };
        let friend_mutation = FriendMutation::SetInconsistent(channel_inconsistent);
        let funder_mutation = FunderMutation::FriendMutation((remote_public_key.clone(), friend_mutation));
        self.apply_funder_mutation(funder_mutation);
    }


    /// Handle success with incoming move token.
    async fn handle_move_token_success(&mut self,
                               remote_public_key: PublicKey,
                               receive_move_token_output: ReceiveMoveTokenOutput<A>,
                               token_wanted: bool) {

        match receive_move_token_output {
            ReceiveMoveTokenOutput::Duplicate => {},
            ReceiveMoveTokenOutput::RetransmitOutgoing(outgoing_move_token) => {
                // Retransmit last sent token channel message:
                self.transmit_outgoing(&remote_public_key);
                // We should not send any new move token in this case:
                return;
            },
            ReceiveMoveTokenOutput::Received(move_token_received) => {
                let MoveTokenReceived {
                    incoming_messages, 
                    mutations, 
                    remote_requests_closed, 
                    opt_local_address
                } = move_token_received;

                // Update address for remote side if necessary:
                if let Some(new_remote_address) = opt_local_address {
                    let friend = self.get_friend(&remote_public_key).unwrap();
                    // Make sure that the newly sent remote address is different than the one we
                    // already have:
                    if friend.remote_address != new_remote_address {
                        // Update remote address:
                        let friend_mutation = FriendMutation::SetRemoteAddress(new_remote_address);
                        let funder_mutation = FunderMutation::FriendMutation((remote_public_key.clone(), friend_mutation));
                        self.apply_funder_mutation(funder_mutation);
                    }
                }

                // Apply all mutations:
                for tc_mutation in mutations {
                    let friend_mutation = FriendMutation::TcMutation(tc_mutation);
                    let funder_mutation = FunderMutation::FriendMutation((remote_public_key.clone(), friend_mutation));
                    self.apply_funder_mutation(funder_mutation);
                }

                // If address update was pending, we can clear it, as this is a proof that the
                // remote side has received our update:
                let friend = self.get_friend(&remote_public_key).unwrap();
                match &friend.sent_local_address {
                    SentLocalAddress::NeverSent |
                    SentLocalAddress::LastSent(_) => {},
                    SentLocalAddress::Transition((last_address, prev_last_address)) => {
                        let friend_mutation = FriendMutation::SetSentLocalAddress(SentLocalAddress::LastSent(last_address.clone()));
                        let funder_mutation = FunderMutation::FriendMutation((remote_public_key.clone(), friend_mutation));
                        self.apply_funder_mutation(funder_mutation);
                    },
                }

                // If remote requests were previously open, and now they were closed:
                if remote_requests_closed {
                    // Cancel all messages pending for this friend. 
                    // We don't want the senders of the requests to wait.
                    await!(self.cancel_pending_requests(
                            remote_public_key.clone()));
                    await!(self.cancel_pending_user_requests(
                            remote_public_key.clone()));
                }

                await!(self.handle_move_token_output(remote_public_key.clone(),
                                               incoming_messages));

            },
        }
        let send_mode = match token_wanted {true => SendMode::EmptyAllowed, false => SendMode::EmptyNotAllowed};
        await!(self.try_send_channel(&remote_public_key, send_mode));
    }


    async fn handle_move_token_request(&mut self, 
                         remote_public_key: PublicKey,
                         friend_move_token_request: MoveTokenRequest<A>) -> Result<(), HandleFriendError> {

        // Find friend:
        let friend = match self.get_friend(&remote_public_key) {
            Some(friend) => Ok(friend),
            None => Err(HandleFriendError::FriendDoesNotExist),
        }?;

        let token_channel = match &friend.channel_status {
            ChannelStatus::Consistent(token_channel) => token_channel,
            ChannelStatus::Inconsistent(channel_inconsistent) => {
                self.try_reset_channel(&remote_public_key, 
                                       &channel_inconsistent.local_reset_terms.clone(),
                                       &friend_move_token_request.friend_move_token);
                return Ok(());
            }
        };

        // We will only consider move token messages if we are in a consistent state:
        let receive_move_token_res = token_channel.simulate_receive_move_token(
            friend_move_token_request.friend_move_token);
        let token_wanted = friend_move_token_request.token_wanted;

        match receive_move_token_res {
            Ok(receive_move_token_output) => {
                await!(self.handle_move_token_success(remote_public_key.clone(),
                                             receive_move_token_output,
                                             token_wanted));
            },
            Err(receive_move_token_error) => {
                await!(self.handle_move_token_error(remote_public_key,
                                             receive_move_token_error));
            },
        };
        Ok(())
    }

    async fn handle_inconsistency_error(&mut self, 
                                  remote_public_key: PublicKey,
                                  remote_reset_terms: ResetTerms)
                                    -> Result<(), HandleFriendError> {

        // Make sure that friend exists:
        let _ = match self.get_friend(&remote_public_key) {
            Some(friend) => Ok(friend),
            None => Err(HandleFriendError::FriendDoesNotExist),
        }?;

        // Cancel all pending requests to this friend:
        await!(self.cancel_pending_requests(
                remote_public_key.clone()));
        await!(self.cancel_pending_user_requests(
                remote_public_key.clone()));

        // Save remote incoming inconsistency details:
        let new_remote_reset_terms = remote_reset_terms;

        // Obtain information about our reset terms:
        let friend = self.get_friend(&remote_public_key).unwrap();
        let (should_send_outgoing, 
             new_local_reset_terms, 
             opt_last_incoming_move_token) = match &friend.channel_status {
            ChannelStatus::Consistent(token_channel) => {
                if !token_channel.is_outgoing() {
                    return Err(HandleFriendError::InconsistencyWhenTokenOwned);
                }
                (true, 
                 await!(get_reset_terms(&token_channel, self.identity_client.clone())),
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
        self.apply_funder_mutation(funder_mutation);

        // Send an outgoing inconsistency message if required:
        if should_send_outgoing {
            self.add_outgoing_comm(FunderOutgoingComm::FriendMessage((remote_public_key.clone(),
                    FriendMessage::InconsistencyError(new_local_reset_terms))));
        }
        Ok(())
    }

    pub async fn handle_friend_message(&mut self, 
                                   remote_public_key: PublicKey, 
                                   friend_message: FriendMessage<A>)
                                        -> Result<(), HandleFriendError> {

        // Make sure that friend exists:
        let _ = match self.get_friend(&remote_public_key) {
            Some(friend) => Ok(friend),
            None => Err(HandleFriendError::FriendDoesNotExist),
        }?;

        match friend_message {
            FriendMessage::MoveTokenRequest(friend_move_token_request) =>
                await!(self.handle_move_token_request(remote_public_key.clone(), friend_move_token_request)),
            FriendMessage::InconsistencyError(remote_reset_terms) => {
                await!(self.handle_inconsistency_error(remote_public_key.clone(), remote_reset_terms))
            }
        }?;

        Ok(())
    }
}
