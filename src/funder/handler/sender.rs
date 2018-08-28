use ring::rand::SecureRandom;

use crypto::identity::PublicKey;
use crypto::rand_values::RandValue;

use super::{FunderTask, FriendMessage, 
    MutableFunderHandler, MAX_MOVE_TOKEN_LENGTH};

use super::super::state::{FunderState, FunderMutation};

use super::super::types::{FriendTcOp, RequestSendFunds, 
    ResponseSendFunds, FailureSendFunds, 
    FriendMoveToken, RequestsStatus};
use super::super::token_channel::outgoing::{OutgoingTokenChannel, QueueOperationFailure,
    QueueOperationError};

use super::super::friend::{FriendMutation, ResponseOp};

use super::super::token_channel::directional::{DirectionalMutation, 
    MoveTokenDirection, SetDirection};


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
        let remote_max_debt = friend
            .directional
            .remote_max_debt();

        if friend.wanted_remote_max_debt != remote_max_debt {
            ops_batch.add(FriendTcOp::SetRemoteMaxDebt(friend.wanted_remote_max_debt))?;
        }

        // Open or close requests is needed:
        let local_requests_status = &friend
            .directional
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
        while let Some(user_request_send_funds) = pending_user_requests.pop_front() {
            let request_op = FriendTcOp::RequestSendFunds(user_request_send_funds.to_request());
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
        let outgoing_move_token = match &friend.directional.direction {
            MoveTokenDirection::Outgoing(outgoing_move_token) => outgoing_move_token.clone(),
            MoveTokenDirection::Incoming(_) => unreachable!(),
        };

        if outgoing_move_token.is_acked {
            return;
        }

        let liveness_friend = self.ephemeral.liveness.friends.get_mut(&remote_public_key).unwrap();
        if liveness_friend.is_online() {
            // Transmit the current outgoing message:
            self.funder_tasks.push(
                FunderTask::FriendMessage(
                    FriendMessage::MoveToken(outgoing_move_token.friend_move_token)));
        }
        liveness_friend.reset_token_msg();
        liveness_friend.cancel_inconsistency();
    }

    pub fn send_friend_move_token(&mut self,
                           remote_public_key: &PublicKey,
                           operations: Vec<FriendTcOp>)
                -> Result<bool, QueueOperationFailure> {


        let friend = self.get_friend(remote_public_key).unwrap();
        let mut out_tc = friend.directional
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
        let friend_move_token = FriendMoveToken {
            operations,
            old_token: friend.directional.new_token().clone(),
            rand_nonce,
        };

        let directional_mutation = DirectionalMutation::SetDirection(
            SetDirection::Outgoing(friend_move_token));
        let friend_mutation = FriendMutation::DirectionalMutation(directional_mutation);
        let messenger_mutation = FunderMutation::FriendMutation((remote_public_key.clone(), friend_mutation));
        self.apply_mutation(messenger_mutation);

        let friend = self.get_friend(remote_public_key).unwrap();
        let outgoing_move_token = friend.directional.get_outgoing_move_token().unwrap();

        let mut move_token_sent = false;

        // Add a task for sending the outgoing move token:
        let liveness_friend = self.ephemeral.liveness.friends.get_mut(&remote_public_key).unwrap();
        if liveness_friend.is_online() {
            // TODO: We only add a FriendMessage::MoveToken task in the case where we think
            // the remote friend is online. Could this cause any problems?
            // Should we add this checks in other places in handle_friend.rs?
            self.add_task(
                FunderTask::FriendMessage(
                    FriendMessage::MoveToken(outgoing_move_token)));
            move_token_sent = true;
        }

        let liveness_friend = self.ephemeral.liveness.friends.get_mut(&remote_public_key).unwrap();
        liveness_friend.reset_token_msg();
        liveness_friend.cancel_inconsistency();
        Ok(move_token_sent)
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
    pub fn send_through_token_channel(&mut self, 
                                  remote_public_key: &PublicKey) -> bool {

        let friend = self.get_friend(remote_public_key).unwrap();
        let mut out_tc = friend.directional
            .begin_outgoing_move_token().unwrap();

        let mut ops_batch = OperationsBatch::new(MAX_MOVE_TOKEN_LENGTH);
        self.queue_outgoing_operations(remote_public_key, &mut ops_batch);
        let operations = ops_batch.done();
        assert!(!operations.is_empty());

        self.send_friend_move_token(
            remote_public_key, operations).unwrap()
    }

    /// Try to send whatever possible through a friend channel.
    /// If we don't own the token, send a request_token message instead.
    pub fn try_send_channel(&mut self,
                        remote_public_key: &PublicKey) {

        let friend = self.get_friend(remote_public_key).unwrap();
        let new_token = friend.directional.new_token();
        match friend.directional.direction {
            MoveTokenDirection::Incoming(_) => {
                // We have the token. 
                // Send as many operations as possible to remote side:
                self.send_through_token_channel(&remote_public_key);
            },
            MoveTokenDirection::Outgoing(_) => {
                // We don't have the token. We should request it.
                let liveness_friend = self.ephemeral.liveness.friends
                    .get_mut(&remote_public_key).unwrap();
                if !liveness_friend.is_request_token_enabled() && liveness_friend.is_online() {
                    self.funder_tasks.push(
                        FunderTask::FriendMessage(
                            FriendMessage::RequestToken(new_token)));
                }
                liveness_friend.enable_request_token();
            },
        };
    }
}
