use ring::rand::SecureRandom;

use crypto::identity::PublicKey;
use crypto::rand_values::RandValue;

use super::{
    MutableFunderHandler, MAX_MOVE_TOKEN_LENGTH};

use super::super::state::{FunderState, FunderMutation};

use super::super::types::{FriendTcOp, RequestSendFunds, 
    ResponseSendFunds, FailureSendFunds, 
    FriendMoveToken, RequestsStatus, FriendMoveTokenRequest,
    FriendMessage, FunderOutgoingComm};
use super::super::token_channel::outgoing::{OutgoingTokenChannel, QueueOperationFailure,
    QueueOperationError};

use super::super::friend::{FriendMutation, ResponseOp, ChannelStatus};

use super::super::token_channel::directional::{DirectionalMutation, 
    MoveTokenDirection, SetDirection};

pub enum SendMode {
    EmptyAllowed,
    EmptyNotAllowed,
}


pub struct OperationsBatch {
    bytes_left: usize,
    operations: Vec<FriendTcOp>,
}

impl OperationsBatch {
    fn new(max_length: usize) -> OperationsBatch {
        OperationsBatch {
            bytes_left: max_length,
            operations: Vec::new(),
        }
    }

    /// queue an operation to the batch of operations.
    /// Make sure that the total length of operations is not too large.
    fn add(&mut self, operation: FriendTcOp) -> Option<()> {
        let op_len = operation.approx_bytes_count();
        let new_bytes_left = self.bytes_left.checked_sub(op_len)?;
        self.bytes_left = new_bytes_left;
        self.operations.push(operation);
        Some(())
    }

    fn done(self) -> Vec<FriendTcOp> {
        self.operations
    }
}




impl<A:Clone,R: SecureRandom> MutableFunderHandler<A,R> {
    /// Queue as many messages as possible into available token channel.
    fn queue_outgoing_operations(&mut self,
                           remote_public_key: &PublicKey,
                           ops_batch: &mut OperationsBatch) -> Option<()> {


        let friend = self.get_friend(remote_public_key).unwrap();

        // Set remote_max_debt if needed:
        let remote_max_debt = match &friend.channel_status {
            ChannelStatus::Consistent(directional) => directional,
            ChannelStatus::Inconsistent(_) => unreachable!(),
        }.remote_max_debt();


        if friend.wanted_remote_max_debt != remote_max_debt {
            ops_batch.add(FriendTcOp::SetRemoteMaxDebt(friend.wanted_remote_max_debt))?;
        }

        let directional = match &friend.channel_status {
            ChannelStatus::Consistent(directional) => directional,
            ChannelStatus::Inconsistent(_) => unreachable!(),
        };

        // Open or close requests is needed:
        let local_requests_status = &directional
            .token_channel
            .state()
            .requests_status
            .local;

        if friend.wanted_local_requests_status != *local_requests_status {
            let friend_op = if let RequestsStatus::Open = friend.wanted_local_requests_status {
                FriendTcOp::EnableRequests
            } else {
                FriendTcOp::DisableRequests
            };
            ops_batch.add(friend_op)?;
        }

        // Send pending responses (responses and failures)
        // TODO: Possibly replace this clone with something more efficient later:
        let mut pending_responses = friend.pending_responses.clone();
        while let Some(pending_response) = pending_responses.pop_front() {
            let pending_op = match pending_response {
                ResponseOp::Response(response) => FriendTcOp::ResponseSendFunds(response),
                ResponseOp::Failure(failure) => FriendTcOp::FailureSendFunds(failure),
            };
            ops_batch.add(pending_op)?;
            let friend_mutation = FriendMutation::PopFrontPendingResponse;
            let messenger_mutation = FunderMutation::FriendMutation((remote_public_key.clone(), friend_mutation));
            self.apply_mutation(messenger_mutation);
        }

        let friend = self.get_friend(remote_public_key).unwrap();

        // Send pending requests:
        // TODO: Possibly replace this clone with something more efficient later:
        let mut pending_requests = friend.pending_requests.clone();
        while let Some(pending_request) = pending_requests.pop_front() {
            let pending_op = FriendTcOp::RequestSendFunds(pending_request);
            ops_batch.add(pending_op)?;
            let friend_mutation = FriendMutation::PopFrontPendingRequest;
            let messenger_mutation = FunderMutation::FriendMutation((remote_public_key.clone(), friend_mutation));
            self.apply_mutation(messenger_mutation);
        }

        let friend = self.get_friend(remote_public_key).unwrap();

        // Send as many pending user requests as possible:
        let mut pending_user_requests = friend.pending_user_requests.clone();
        while let Some(request_send_funds) = pending_user_requests.pop_front() {
            let request_op = FriendTcOp::RequestSendFunds(request_send_funds);
            ops_batch.add(request_op)?;
            let friend_mutation = FriendMutation::PopFrontPendingUserRequest;
            let messenger_mutation = FunderMutation::FriendMutation((remote_public_key.clone(), friend_mutation));
            self.apply_mutation(messenger_mutation);
        }
        Some(())
    }

    /// Transmit the current outgoing friend_move_token.
    pub fn transmit_outgoing(&mut self,
                               remote_public_key: &PublicKey) {

        let friend = self.get_friend(remote_public_key).unwrap();
        let directional = match &friend.channel_status {
            ChannelStatus::Consistent(directional) => directional,
            ChannelStatus::Inconsistent(_) => unreachable!(),
        };

        let friend_move_token_request = match &directional.direction {
            MoveTokenDirection::Outgoing(friend_move_token_request) => friend_move_token_request.clone(),
            MoveTokenDirection::Incoming(_) => unreachable!(),
        };

        // Transmit the current outgoing message:
        self.add_outgoing_comm(FunderOutgoingComm::FriendMessage(
            (remote_public_key.clone(),
                FriendMessage::MoveTokenRequest(friend_move_token_request))));
    }

    fn send_friend_move_token(&mut self,
                           remote_public_key: &PublicKey,
                           operations: Vec<FriendTcOp>)
                -> Result<(), QueueOperationFailure> {


        let friend = self.get_friend(remote_public_key).unwrap();
        let directional = match &friend.channel_status {
            ChannelStatus::Consistent(directional) => directional,
            ChannelStatus::Inconsistent(_) => unreachable!(),
        };
        let mut out_tc = directional
            .begin_outgoing_move_token().unwrap();

        for op in operations {
            out_tc.queue_operation(op)?;
        }

        let (operations, tc_mutations) = out_tc.done();

        for tc_mutation in tc_mutations {
            let directional_mutation = DirectionalMutation::TcMutation(tc_mutation);
            let friend_mutation = FriendMutation::DirectionalMutation(directional_mutation);
            let messenger_mutation = FunderMutation::FriendMutation((remote_public_key.clone(), friend_mutation));
            self.apply_mutation(messenger_mutation);
        }

        // Update freeze guard about outgoing requests:
        for operation in &operations {
            if let FriendTcOp::RequestSendFunds(request_send_funds) = operation {
                self.ephemeral.freeze_guard
                    .add_frozen_credit(
                        &request_send_funds.create_pending_request());
            }
        }

        let friend = self.get_friend(remote_public_key).unwrap();

        let rand_nonce = RandValue::new(&*self.rng);
        let directional = match &friend.channel_status {
            ChannelStatus::Consistent(directional) => directional,
            ChannelStatus::Inconsistent(_) => unreachable!(),
        };
        let friend_move_token = FriendMoveToken {
            operations,
            old_token: directional.new_token().clone(),
            rand_nonce,
        };

        let directional_mutation = DirectionalMutation::SetDirection(
            SetDirection::Outgoing(friend_move_token));
        let friend_mutation = FriendMutation::DirectionalMutation(directional_mutation);
        let messenger_mutation = FunderMutation::FriendMutation((remote_public_key.clone(), friend_mutation));
        self.apply_mutation(messenger_mutation);

        let friend = self.get_friend(remote_public_key).unwrap();
        let directional = match &friend.channel_status {
            ChannelStatus::Consistent(directional) => directional,
            ChannelStatus::Inconsistent(_) => unreachable!(),
        };
        let friend_move_token_request = 
            directional.get_outgoing_move_token().unwrap();

        // Add a task for sending the outgoing move token:
        self.add_outgoing_comm(FunderOutgoingComm::FriendMessage(
            (remote_public_key.clone(),
                FriendMessage::MoveTokenRequest(friend_move_token_request))));

        Ok(())
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
    fn send_through_token_channel(&mut self, 
                                  remote_public_key: &PublicKey,
                                  send_mode: SendMode) -> bool {

        let friend = self.get_friend(remote_public_key).unwrap();
        let directional = match &friend.channel_status {
            ChannelStatus::Consistent(directional) => directional,
            ChannelStatus::Inconsistent(_) => unreachable!(),
        };
        let mut out_tc = directional
            .begin_outgoing_move_token().unwrap();

        let mut ops_batch = OperationsBatch::new(MAX_MOVE_TOKEN_LENGTH);
        self.queue_outgoing_operations(remote_public_key, &mut ops_batch);
        let operations = ops_batch.done();

        let may_send_empty = if let SendMode::EmptyAllowed = send_mode {true} else {false};
        if may_send_empty || !operations.is_empty() {
            self.send_friend_move_token(
                remote_public_key, operations).unwrap();
            true
        } else {
            false
        }
    }

    /// Try to send whatever possible through a friend channel.
    pub fn try_send_channel(&mut self,
                        remote_public_key: &PublicKey,
                        send_mode: SendMode) {

        let friend = self.get_friend(remote_public_key).unwrap();

        // We do not send messages if we are in an inconsistent status:
        let directional = match &friend.channel_status {
            ChannelStatus::Consistent(directional) => directional,
            ChannelStatus::Inconsistent(_) => return,
        };

        match &directional.direction {
            MoveTokenDirection::Incoming(_) => {
                // We have the token. 
                // Send as many operations as possible to remote side:
                self.send_through_token_channel(&remote_public_key, send_mode);
            },
            MoveTokenDirection::Outgoing(outgoing_move_token) => {
                if !outgoing_move_token.token_wanted {
                    // We don't have the token. We should request it.
                    // Mark that we have sent a request token, to make sure we don't do this again:
                    let directional_mutation = DirectionalMutation::SetTokenWanted;
                    let friend_mutation = FriendMutation::DirectionalMutation(directional_mutation);
                    let messenger_mutation = FunderMutation::FriendMutation((remote_public_key.clone(), friend_mutation));
                    self.apply_mutation(messenger_mutation);

                    // TODO: Should we set outgoing_move_token.token_wanted = true here?

                    self.transmit_outgoing(remote_public_key);
                }
            },
        };
    }
}
