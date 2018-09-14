use std::convert::TryFrom;

use futures::prelude::{async, await};

use num_bigint::BigUint;
use num_traits::ToPrimitive;

use ring::rand::SecureRandom;

use crypto::rand_values::RandValue;
use crypto::identity::{PublicKey, Signature};
use crypto::uid::Uid;

use utils::safe_arithmetic::SafeArithmetic;


use super::super::token_channel::incoming::{IncomingResponseSendFunds, 
    IncomingFailureSendFunds, IncomingMessage};
use super::super::token_channel::outgoing::{OutgoingTokenChannel, QueueOperationFailure,
    QueueOperationError};
use super::super::token_channel::directional::{ReceiveMoveTokenOutput, ReceiveMoveTokenError, 
    DirectionalMutation, MoveTokenDirection, MoveTokenReceived, SetDirection};
use super::{MutableFunderHandler, FriendMoveTokenRequest};
use super::super::types::{RequestSendFunds, ResponseSendFunds, 
    FailureSendFunds, FriendMoveToken, 
    FunderFreezeLink, PkPairPosition, 
    PendingFriendRequest, Ratio, RequestsStatus, SendFundsReceipt,
    ChannelToken, FriendInconsistencyError,
    FriendMessage, ResponseReceived, ResetTerms,
    FunderOutgoingControl, FunderOutgoingComm};

use super::super::state::FunderMutation;
use super::super::friend::{FriendState, FriendMutation, 
    ResponseOp, ChannelStatus};

use super::super::signature_buff::{create_failure_signature_buffer, 
    create_response_signature_buffer, prepare_receipt};
use super::super::messages::ResponseSendFundsResult;
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


#[allow(unused)]
impl<A: Clone + 'static, R: SecureRandom + 'static> MutableFunderHandler<A,R> {

    /// Check if channel reset is required (Remove side used the RESET token)
    /// If so, reset the channel.
    fn try_reset_channel(&mut self, 
                           friend_public_key: &PublicKey,
                           local_reset_terms: &ResetTerms,
                           friend_move_token: &FriendMoveToken) {

        // Check if incoming message is an attempt to reset channel.
        // We can know this by checking if old_token is a special value.
        if friend_move_token.old_token == local_reset_terms.reset_token {
            // This is a reset message. We reset the token channel:
            let friend_mutation = FriendMutation::RemoteReset;
            let messenger_mutation = FunderMutation::FriendMutation((friend_public_key.clone(), friend_mutation));
            self.apply_mutation(messenger_mutation);
        }

    }

    pub fn add_local_freezing_link(&self, request_send_funds: &mut RequestSendFunds) {
        let index = request_send_funds.route.pk_to_index(self.state.get_local_public_key())
            .unwrap();
        assert_eq!(request_send_funds.freeze_links.len(), index);
        let next_index = index.checked_add(1).unwrap();
        let next_pk = request_send_funds.route.index_to_pk(next_index).unwrap();
        let next_friend = self.state.get_friends().get(&next_pk).unwrap();
        let next_directional = match &next_friend.channel_status {
            ChannelStatus::Consistent(directional) => directional,
            ChannelStatus::Inconsistent(_) => unreachable!(),
        };
        let forward_trust: BigUint = next_directional.token_channel.state().balance.remote_max_debt.into();
        let total_trust = self.state.get_total_trust();
        let two_pow_128 = BigUint::new(vec![0x1, 0x0u32, 0x0u32, 0x0u32, 0x0u32]);

        let funder_freeze_link = if index == 0 {
            // We are the first node on the route (We initiated this request):
            
            let numerator = (two_pow_128 * forward_trust) / &total_trust;
            let usable_ratio = match numerator.to_u128() {
                Some(num) => Ratio::Numerator(num),
                None => Ratio::One,
            };

            FunderFreezeLink {
                shared_credits: total_trust.to_u128().unwrap_or(u128::max_value()),
                usable_ratio,
            }

        } else {
            // We are not the first node on the route:
            let prev_index = index.checked_sub(1).unwrap();
            let prev_pk = request_send_funds.route.index_to_pk(prev_index).unwrap();
            let prev_friend = self.state.get_friends().get(&prev_pk).unwrap();
            let prev_directional = match &prev_friend.channel_status {
                ChannelStatus::Consistent(directional) => directional,
                ChannelStatus::Inconsistent(_) => unreachable!(),
            };

            let prev_trust: BigUint = prev_directional.token_channel.state().balance.remote_max_debt.into();
            let numerator = (two_pow_128 * forward_trust) / (total_trust - &prev_trust);
            let usable_ratio = match numerator.to_u128() {
                Some(num) => Ratio::Numerator(num),
                None => Ratio::One,
            };

            let shared_credits = prev_trust.to_u128().unwrap_or(u128::max_value());
            FunderFreezeLink {
                shared_credits,
                usable_ratio,
            }

        };

        // Add our freeze link
        request_send_funds.freeze_links.push(funder_freeze_link);

    }

    /// Forward a request message to the relevant friend and token channel.
    fn forward_request(&mut self, mut request_send_funds: RequestSendFunds) {
        let index = request_send_funds.route.pk_to_index(self.state.get_local_public_key())
            .unwrap();
        let next_index = index.checked_add(1).unwrap();
        let next_pk = request_send_funds.route.index_to_pk(next_index).unwrap();

        // Queue message to the relevant friend. Later this message will be queued to a specific
        // available token channel:
        let friend_mutation = FriendMutation::PushBackPendingRequest(request_send_funds.clone());
        let messenger_mutation = FunderMutation::FriendMutation((next_pk.clone(), friend_mutation));
        self.apply_mutation(messenger_mutation);
        self.try_send_channel(&next_pk, SendMode::EmptyNotAllowed);
    }

    /// Create a (signed) failure message for a given request_id.
    /// We are the reporting_public_key for this failure message.
    #[async]
    fn create_response_message(self, request_send_funds: RequestSendFunds) 
        -> Result<(Self, ResponseSendFunds), !> {

        let rand_nonce = RandValue::new(&*self.rng);
        let local_public_key = self.state.get_local_public_key().clone();

        let mut response_send_funds = ResponseSendFunds {
            request_id: request_send_funds.request_id,
            rand_nonce,
            signature: Signature::zero(),
        };

        let response_signature_buffer = create_response_signature_buffer(&response_send_funds,
                        &request_send_funds.create_pending_request());

        response_send_funds.signature = await!(self.identity_client.request_signature(response_signature_buffer))
            .unwrap();

        Ok((self, response_send_funds))
    }

    #[async]
    fn handle_request_send_funds(mut self, 
                               remote_public_key: PublicKey,
                               mut request_send_funds: RequestSendFunds) -> Result<Self, !> {

        // Find ourselves on the route. If we are not there, abort.
        let remote_index = request_send_funds.route.find_pk_pair(
            &remote_public_key, 
            self.state.get_local_public_key()).unwrap();

        let local_index = remote_index.checked_add(1).unwrap();
        let next_index = local_index.checked_add(1).unwrap();
        if next_index >= request_send_funds.route.len() {
            return Ok(self);
        }


        // The node on the route has to be one of our friends:
        let next_public_key = request_send_funds.route.index_to_pk(next_index).unwrap();
        let friend_exists = !self.state.get_friends().contains_key(next_public_key);

        // This friend must be considered online for us to forward the message.
        // If we forward the request to an offline friend, the request could be stuck for a long
        // time before a response arrives.
        let friend_ready = if friend_exists {
            self.is_friend_ready(&next_public_key)
        } else {
            false
        };

        let mut fself = if !friend_ready {
            await!(self.reply_with_failure(remote_public_key.clone(), 
                                           request_send_funds.clone()))?
        } else {
            self
        };

        // Add our freezing link:
        fself.add_local_freezing_link(&mut request_send_funds);

        // Perform DoS protection check:
        Ok(match fself.ephemeral.freeze_guard.verify_freezing_links(&request_send_funds) {
            Some(()) => {
                // Add our freezing link, and queue message to the next node.
                fself.forward_request(request_send_funds);
                fself
            },
            None => {
                // Queue a failure message to this token channel:
                await!(fself.reply_with_failure(remote_public_key, 
                                               request_send_funds))?
            },
        })
    }


    fn handle_response_send_funds(&mut self, 
                               remote_public_key: &PublicKey,
                               response_send_funds: ResponseSendFunds,
                               pending_request: PendingFriendRequest) {

        match self.find_request_origin(&response_send_funds.request_id).cloned() {
            None => {
                // We are the origin of this request, and we got a response.
                // We should pass it back to crypter.


                let receipt = prepare_receipt(&response_send_funds,
                                              &pending_request);

                let response_send_funds_result = ResponseSendFundsResult::Success(receipt);
                self.add_response_received(
                    ResponseReceived {
                        request_id: pending_request.request_id,
                        result: response_send_funds_result,
                    }
                );
            },
            Some(friend_public_key) => {
                // Queue this response message to another token channel:
                let response_op = ResponseOp::Response(response_send_funds);
                let friend_mutation = FriendMutation::PushBackPendingResponse(response_op);
                let messenger_mutation = FunderMutation::FriendMutation((friend_public_key.clone(), friend_mutation));
                self.apply_mutation(messenger_mutation);
                self.try_send_channel(&friend_public_key, SendMode::EmptyNotAllowed);
            },
        }
    }

    #[async]
    fn handle_failure_send_funds(mut self, 
                               remote_public_key: &PublicKey,
                               failure_send_funds: FailureSendFunds,
                               pending_request: PendingFriendRequest)
                                -> Result<Self, !> {

        let fself = match self.find_request_origin(&failure_send_funds.request_id).cloned() {
            None => {
                // We are the origin of this request, and we got a failure
                // We should pass it back to crypter.


                let response_send_funds_result = ResponseSendFundsResult::Failure(failure_send_funds.reporting_public_key);
                self.add_response_received(
                    ResponseReceived {
                        request_id: pending_request.request_id,
                        result: response_send_funds_result,
                    }
                );

                self
            },
            Some(friend_public_key) => {
                // Queue this failure message to another token channel:
                let failure_op = ResponseOp::Failure(failure_send_funds);
                let friend_mutation = FriendMutation::PushBackPendingResponse(failure_op);
                let messenger_mutation = FunderMutation::FriendMutation((friend_public_key.clone(), friend_mutation));
                self.apply_mutation(messenger_mutation);
                self.try_send_channel(&friend_public_key, SendMode::EmptyNotAllowed);

                self
            },
        };
        Ok(fself)
    }

    /// Process valid incoming operations from remote side.
    #[async]
    fn handle_move_token_output(mut self, 
                                remote_public_key: PublicKey,
                                incoming_messages: Vec<IncomingMessage> )
                        -> Result<Self, !> {

        let mut fself = self;
        for incoming_message in incoming_messages {
            fself = match incoming_message {
                IncomingMessage::Request(request_send_funds) => {
                    await!(fself.handle_request_send_funds(remote_public_key.clone(),
                                                 request_send_funds))?
                },
                IncomingMessage::Response(IncomingResponseSendFunds {
                                                pending_request, incoming_response}) => {
                    fself.ephemeral.freeze_guard.sub_frozen_credit(&pending_request);
                    fself.handle_response_send_funds(&remote_public_key, 
                                                  incoming_response, pending_request);
                    fself
                },
                IncomingMessage::Failure(IncomingFailureSendFunds {
                                                pending_request, incoming_failure}) => {
                    fself.ephemeral.freeze_guard.sub_frozen_credit(&pending_request);
                    await!(fself.handle_failure_send_funds(&remote_public_key, 
                                                 incoming_failure, pending_request))?
                },
            }
        }
        Ok(fself)
    }

    /// Handle an error with incoming move token.
    #[async]
    fn handle_move_token_error(mut self,
                               remote_public_key: PublicKey,
                               receive_move_token_error: ReceiveMoveTokenError) -> Result<Self, !> {

        let friend = self.get_friend(&remote_public_key).unwrap();
        let directional = match &friend.channel_status {
            ChannelStatus::Consistent(directional) => directional,
            ChannelStatus::Inconsistent(_) => unreachable!(),
        };
        // Send an InconsistencyError message to remote side:
        let local_reset_terms = directional.get_reset_terms();

        self.add_outgoing_comm(FunderOutgoingComm::FriendMessage((remote_public_key.clone(),
                FriendMessage::InconsistencyError(local_reset_terms.clone()))));

        // Keep outgoing InconsistencyError message details in memory:
        let friend_mutation = FriendMutation::SetChannelStatus((local_reset_terms, None));
        let messenger_mutation = FunderMutation::FriendMutation((remote_public_key.clone(), friend_mutation));
        self.apply_mutation(messenger_mutation);


        // Cancel all internal pending requests inside token channel:
        let fself = await!(self.cancel_local_pending_requests(
            remote_public_key.clone()))?;
        // Cancel all pending requests to this friend:
        let fself = await!(fself.cancel_pending_requests(
                remote_public_key.clone()))?;
        let fself = await!(fself.cancel_pending_user_requests(
                remote_public_key.clone()))?;

        Ok(fself)
    }


    /// Handle success with incoming move token.
    #[async]
    fn handle_move_token_success(mut self,
                               remote_public_key: PublicKey,
                               receive_move_token_output: ReceiveMoveTokenOutput,
                               token_wanted: bool) 
                                -> Result<Self, !> {

        match receive_move_token_output {
            ReceiveMoveTokenOutput::Duplicate => Ok(self),
            ReceiveMoveTokenOutput::RetransmitOutgoing(outgoing_move_token) => {
                // Retransmit last sent token channel message:
                self.transmit_outgoing(&remote_public_key);
                Ok(self)
            },
            ReceiveMoveTokenOutput::Received(move_token_received) => {
                let MoveTokenReceived {incoming_messages, mutations} = 
                    move_token_received;

                // Apply all mutations:
                for directional_mutation in mutations {
                    let friend_mutation = FriendMutation::DirectionalMutation(directional_mutation);
                    let messenger_mutation = FunderMutation::FriendMutation((remote_public_key.clone(), friend_mutation));
                    self.apply_mutation(messenger_mutation);
                }

                let mut fself = await!(self.handle_move_token_output(remote_public_key.clone(),
                                               incoming_messages))?;
                let send_mode = match token_wanted {true => SendMode::EmptyAllowed, false => SendMode::EmptyNotAllowed};
                fself.try_send_channel(&remote_public_key, send_mode);
                Ok(fself)
            },
        }
    }


    #[async]
    fn handle_move_token_request(mut self, 
                         remote_public_key: PublicKey,
                         friend_move_token_request: FriendMoveTokenRequest) -> Result<Self,HandleFriendError> {

        // Find friend:
        let friend = match self.get_friend(&remote_public_key) {
            Some(friend) => Ok(friend),
            None => Err(HandleFriendError::FriendDoesNotExist),
        }?;

        let directional = match &friend.channel_status {
            ChannelStatus::Consistent(directional) => directional,
            ChannelStatus::Inconsistent((local_reset_terms, _)) => {
                self.try_reset_channel(&remote_public_key, 
                                       &local_reset_terms.clone(),
                                       &friend_move_token_request.friend_move_token);
                return Ok(self);
            }
        };

        // We will only consider move token messages if we are in a consistent state:
        let receive_move_token_res = directional.simulate_receive_move_token(
            friend_move_token_request.friend_move_token);
        let token_wanted = friend_move_token_request.token_wanted;

        Ok(match receive_move_token_res {
            Ok(receive_move_token_output) => {
                await!(self.handle_move_token_success(remote_public_key.clone(),
                                             receive_move_token_output,
                                             token_wanted))?
            },
            Err(receive_move_token_error) => {
                await!(self.handle_move_token_error(remote_public_key,
                                             receive_move_token_error))?
            },
        })
    }

    #[async]
    fn handle_inconsistency_error(self, 
                                  remote_public_key: PublicKey,
                                  remote_reset_terms: ResetTerms)
                                    -> Result<Self, HandleFriendError> {

        // Make sure that friend exists:
        let _ = match self.get_friend(&remote_public_key) {
            Some(friend) => Ok(friend),
            None => Err(HandleFriendError::FriendDoesNotExist),
        }?;

        // Cancel all pending requests to this friend:
        let fself = await!(self.cancel_pending_requests(
                remote_public_key.clone()))?;
        let mut fself = await!(fself.cancel_pending_user_requests(
                remote_public_key.clone()))?;

        // Save remote incoming inconsistency details:
        let new_remote_reset_terms = remote_reset_terms;

        // Obtain information about our reset terms:
        let friend = fself.get_friend(&remote_public_key).unwrap();
        let (should_send_outgoing, new_local_reset_terms) = match &friend.channel_status {
            ChannelStatus::Consistent(directional) => {
                if !directional.is_outgoing() {
                    return Err(HandleFriendError::InconsistencyWhenTokenOwned);
                }
                (true, directional.get_reset_terms())
            },
            ChannelStatus::Inconsistent((local_reset_terms, _)) => (false, local_reset_terms.clone()),
        };


        let friend_mutation = FriendMutation::SetChannelStatus(
            (new_local_reset_terms.clone(), Some(new_remote_reset_terms)));
        let messenger_mutation = FunderMutation::FriendMutation((remote_public_key.clone(), friend_mutation));
        fself.apply_mutation(messenger_mutation);

        // Send an outgoing inconsistency message if required:
        if should_send_outgoing {
            fself.add_outgoing_comm(FunderOutgoingComm::FriendMessage((remote_public_key.clone(),
                    FriendMessage::InconsistencyError(new_local_reset_terms))));
        }
        Ok(fself)
    }

    #[async]
    pub fn handle_friend_message(mut self, 
                                   remote_public_key: PublicKey, 
                                   friend_message: FriendMessage)
                                        -> Result<Self, HandleFriendError> {

        // Make sure that friend exists:
        let _ = match self.get_friend(&remote_public_key) {
            Some(friend) => Ok(friend),
            None => Err(HandleFriendError::FriendDoesNotExist),
        }?;

        let mut fself = match friend_message {
            FriendMessage::MoveTokenRequest(friend_move_token_request) =>
                await!(self.handle_move_token_request(remote_public_key.clone(), friend_move_token_request)),
            FriendMessage::InconsistencyError(remote_reset_terms) => {
                await!(self.handle_inconsistency_error(remote_public_key.clone(), remote_reset_terms))
            }
        }?;

        Ok(fself)
    }
}
