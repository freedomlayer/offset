#![allow(unused)]
use futures::prelude::{async, await};
use ring::rand::SecureRandom;

use crypto::identity::PublicKey;
use crypto::uid::Uid;
use crypto::hash::HashResult;
use crypto::rand_values::RandValue;

use proto::funder::{ChannelToken, InvoiceId};

use super::super::token_channel::types::TcMutation;
use super::super::token_channel::directional::{DirectionalMutation, 
    OutgoingMoveToken, MoveTokenDirection};
use super::super::friend::{FriendState, FriendMutation, IncomingInconsistency};
use super::super::state::{FunderMutation, FunderState};
use super::{MutableFunderHandler, FunderTask, ResponseReceived, 
    FriendMessage, MAX_MOVE_TOKEN_LENGTH};
use super::super::messages::ResponseSendFundsResult;
use super::super::liveness::Direction;
use super::super::types::{RequestsStatus, FriendStatus, UserRequestSendFunds,
    SetFriendRemoteMaxDebt, ResetFriendChannel,
    SetFriendAddr, AddFriend, RemoveFriend, SetFriendStatus, SetRequestsStatus, 
    ReceiptAck, FriendsRoute, FriendMoveToken, IncomingControlMessage,
    FriendTcOp};

const MAX_PENDING_USER_REQUESTS: usize = 0x10;

pub enum HandleControlError {
    FriendDoesNotExist,
    TokenChannelDoesNotExist,
    NotInvitedToReset,
    ResetTokenMismatch,
    NotFirstInRoute,
    InvalidRoute,
    RequestAlreadyInProgress,
    PendingUserRequestsFull,
    ReceiptDoesNotExist,
    UserRequestInvalid,
}


#[allow(unused)]
impl<A:Clone + 'static, R: SecureRandom + 'static> MutableFunderHandler<A,R> {

    fn control_set_friend_remote_max_debt(&mut self, 
                                            set_friend_remote_max_debt: SetFriendRemoteMaxDebt) 
        -> Result<(), HandleControlError> {

        // Make sure that friend exists:
        let _friend = self.get_friend(&set_friend_remote_max_debt.friend_public_key)
            .ok_or(HandleControlError::FriendDoesNotExist)?;

        let tc_mutation = TcMutation::SetRemoteMaxDebt(set_friend_remote_max_debt.remote_max_debt);
        let directional_mutation = DirectionalMutation::TcMutation(tc_mutation);
        let friend_mutation = FriendMutation::DirectionalMutation(directional_mutation);
        let m_mutation = FunderMutation::FriendMutation(
            (set_friend_remote_max_debt.friend_public_key, friend_mutation));

        self.apply_mutation(m_mutation);
        Ok(())
    }

    #[async]
    fn control_reset_friend_channel(mut self, 
                                    reset_friend_channel: ResetFriendChannel) 
        -> Result<Self, HandleControlError> {

        let friend = self.get_friend(&reset_friend_channel.friend_public_key)
            .ok_or(HandleControlError::FriendDoesNotExist)?;

        let reset_terms = match &friend.inconsistency_status.incoming {
            IncomingInconsistency::Empty => return Err(HandleControlError::NotInvitedToReset),
            IncomingInconsistency::Incoming(reset_terms) => {
                if (reset_terms.current_token != reset_friend_channel.current_token)  {
                    return Err(HandleControlError::ResetTokenMismatch);
                }
                reset_terms
            }
        };

        let rand_nonce = RandValue::new(&*self.rng);
        let friend_move_token = FriendMoveToken {
            operations: Vec::new(), 
            // No operations are required for a reset move token
            old_token: reset_terms.current_token.clone(),
            rand_nonce,
        };

        let mut fself = await!(self.cancel_local_pending_requests(
            reset_friend_channel.friend_public_key.clone()))?;

        let friend_mutation = FriendMutation::LocalReset(friend_move_token.clone());
        let m_mutation = FunderMutation::FriendMutation(
            (reset_friend_channel.friend_public_key.clone(), friend_mutation));
        fself.apply_mutation(m_mutation);

        fself.transmit_outgoing(&reset_friend_channel.friend_public_key);

        Ok(fself)

    }

    fn control_add_friend(&mut self, add_friend: AddFriend<A>) 
        -> Result<(), HandleControlError> {

        let m_mutation = FunderMutation::AddFriend((
                add_friend.friend_public_key.clone(),
                add_friend.address));

        self.apply_mutation(m_mutation);

        let friend = self.get_friend(&add_friend.friend_public_key).unwrap();
        let direction = match &friend.directional.direction {
            MoveTokenDirection::Incoming(_) => Direction::Incoming,
            MoveTokenDirection::Outgoing(outgoing_move_token) => 
                Direction::Outgoing(outgoing_move_token.is_acked)
        };
        self.ephemeral.liveness.add_friend(&add_friend.friend_public_key,
                                           direction);

        Ok(())
    }

    #[async]
    /// This is a violent operation, as it removes all the known state with the remote friend.  
    /// An inconsistency will occur if the friend is added again.
    fn control_remove_friend(mut self, remove_friend: RemoveFriend) 
        -> Result<Self, HandleControlError> {

        // Make sure that friend exists:
        let _friend = self.get_friend(&remove_friend.friend_public_key)
            .ok_or(HandleControlError::FriendDoesNotExist)?;

        let fself = await!(self.cancel_local_pending_requests(
            remove_friend.friend_public_key.clone()))?;
        let fself = await!(fself.cancel_pending_requests(
                remove_friend.friend_public_key.clone()))?;
        let mut fself = await!(fself.cancel_pending_user_requests(
                remove_friend.friend_public_key.clone()))?;

        let m_mutation = FunderMutation::RemoveFriend(
                remove_friend.friend_public_key.clone());

        fself.apply_mutation(m_mutation);
        fself.ephemeral.liveness.remove_friend(&remove_friend.friend_public_key);
                                               
        Ok(fself)
    }


    fn control_set_friend_status(&mut self, set_friend_status: SetFriendStatus) 
        -> Result<(), HandleControlError> {

        // Make sure that friend exists:
        let _friend = self.get_friend(&set_friend_status.friend_public_key)
            .ok_or(HandleControlError::FriendDoesNotExist)?;

        let friend_mutation = FriendMutation::SetStatus(set_friend_status.status);
        let m_mutation = FunderMutation::FriendMutation(
            (set_friend_status.friend_public_key, friend_mutation));

        self.apply_mutation(m_mutation);
        Ok(())
    }

    fn control_set_requests_status(&mut self, set_requests_status: SetRequestsStatus) 
        -> Result<(), HandleControlError> {

        // Make sure that friend exists:
        let _friend = self.get_friend(&set_requests_status.friend_public_key)
            .ok_or(HandleControlError::FriendDoesNotExist)?;

        let friend_mutation = FriendMutation::SetWantedLocalRequestsStatus(set_requests_status.status);
        let m_mutation = FunderMutation::FriendMutation(
            (set_requests_status.friend_public_key, friend_mutation));

        self.apply_mutation(m_mutation);
        Ok(())
    }

    fn control_set_friend_addr(&mut self, set_friend_addr: SetFriendAddr<A>) 
        -> Result<(), HandleControlError> {

        // Make sure that friend exists:
        let _friend = self.get_friend(&set_friend_addr.friend_public_key)
            .ok_or(HandleControlError::FriendDoesNotExist)?;

        let friend_mutation = FriendMutation::SetFriendAddr(set_friend_addr.address);
        let m_mutation = FunderMutation::FriendMutation(
            (set_friend_addr.friend_public_key, friend_mutation));

        self.apply_mutation(m_mutation);
        Ok(())
    }

    fn check_user_request_valid(&self, 
                                user_request_send_funds: &UserRequestSendFunds) 
                                -> Option<()> {

        let friend_op = FriendTcOp::RequestSendFunds(user_request_send_funds.clone().to_request());
        MAX_MOVE_TOKEN_LENGTH.checked_sub(friend_op.approx_bytes_count())?;
        Some(())
    }

    fn control_request_send_funds_inner(&mut self, user_request_send_funds: UserRequestSendFunds)
        -> Result<(), HandleControlError> {

        self.check_user_request_valid(&user_request_send_funds)
            .ok_or(HandleControlError::UserRequestInvalid)?;


        // If we already have a receipt for this request, we return the receipt immediately and
        // exit. Note that we don't erase the receipt yet. This will only be done when a receipt
        // ack is received.
        if let Some(receipt) = self.state.ready_receipts.get(&user_request_send_funds.request_id) {
            let response_received = ResponseReceived {
                request_id: user_request_send_funds.request_id,
                result: ResponseSendFundsResult::Success(receipt.clone()),
            };
            self.funder_tasks.push(FunderTask::ResponseReceived(response_received));
            return Ok(());
        }

        let route = &user_request_send_funds.route;

        // We have to be the first on the route:
        match route.public_keys.first() {
            Some(first) if *first == self.state.local_public_key => Ok(()),
            _ => Err(HandleControlError::NotFirstInRoute),
        }?;

        // We want to have at least two public keys on the route (source and destination).
        // We also want that the public keys on the route are unique.
        if (route.len() < 2) || !route.is_cycle_free() {
            return Err(HandleControlError::InvalidRoute);
        }
        let friend_public_key = &route.public_keys[1];

        let friend = match self.get_friend(friend_public_key) {
            Some(friend) => Ok(friend),
            None => Err(HandleControlError::FriendDoesNotExist),
        }?;

        // If request is already in progress, we do nothing:
        // Check if there is already a pending user request with the same request_id:
        for user_request in &friend.pending_user_requests {
            if user_request_send_funds.request_id == user_request.request_id {
                return Err(HandleControlError::RequestAlreadyInProgress);
            }
        }

        // Check if there is an onging request with the same request_id with this specific friend:
        if friend.directional
            .token_channel
            .state()
            .pending_requests
            .pending_local_requests
            .contains_key(&user_request_send_funds.request_id) {
                return Err(HandleControlError::RequestAlreadyInProgress);
        }

        // Check if we have room to push this message:
        if friend.pending_user_requests.len() >= MAX_PENDING_USER_REQUESTS {
            return Err(HandleControlError::PendingUserRequestsFull);
        }

        let friend_mutation = FriendMutation::PushBackPendingUserRequest(user_request_send_funds.clone());
        let messenger_mutation = FunderMutation::FriendMutation((friend_public_key.clone(), friend_mutation));
        self.apply_mutation(messenger_mutation);
        self.try_send_channel(&friend_public_key);

        Ok(())
    }


    fn control_request_send_funds(&mut self, user_request_send_funds: UserRequestSendFunds) 
        -> Result<(), HandleControlError> {
        
        // If we managed to push the message, we return an Ok(()).
        // Otherwise, we return the internal error and return a response failure message.
        self.control_request_send_funds_inner(user_request_send_funds.clone())
            .map_err(|e| {
                let response_received = ResponseReceived {
                    request_id: user_request_send_funds.request_id,
                    result: ResponseSendFundsResult::Failure(self.state.local_public_key.clone()),
                };
                self.funder_tasks.push(FunderTask::ResponseReceived(response_received));
                e
            })
    }

    /// Handle an incoming receipt ack message
    fn control_receipt_ack(&mut self, receipt_ack: ReceiptAck) 
        -> Result<(), HandleControlError> {

        if !self.state.ready_receipts.contains_key(&receipt_ack.request_id) {
            return Err(HandleControlError::ReceiptDoesNotExist);
        }
        let m_mutation = FunderMutation::RemoveReceipt(receipt_ack.request_id);
        self.apply_mutation(m_mutation);

        Ok(())
    }


    #[async]
    pub fn handle_control_message(mut self, 
                                  funder_config: IncomingControlMessage<A>) 
        -> Result<Self, HandleControlError> {


        let fself = match funder_config {
            IncomingControlMessage::SetFriendRemoteMaxDebt(set_friend_remote_max_debt) => {
                self.control_set_friend_remote_max_debt(set_friend_remote_max_debt);
                self
            },
            IncomingControlMessage::ResetFriendChannel(reset_friend_channel) => {
                let fself = await!(self.control_reset_friend_channel(reset_friend_channel))?;
                fself
            },
            IncomingControlMessage::AddFriend(add_friend) => {
                self.control_add_friend(add_friend);
                self
            },
            IncomingControlMessage::RemoveFriend(remove_friend) => {
                let fself = await!(self.control_remove_friend(remove_friend))?;
                fself
            },
            IncomingControlMessage::SetFriendStatus(set_friend_status) => {
                self.control_set_friend_status(set_friend_status);
                self
            },
            IncomingControlMessage::SetRequestsStatus(set_requests_status) => {
                self.control_set_requests_status(set_requests_status);
                self
            },
            IncomingControlMessage::SetFriendAddr(set_friend_addr) => {
                self.control_set_friend_addr(set_friend_addr);
                self
            },
            IncomingControlMessage::RequestSendFunds(user_request_send_funds) => {
                self.control_request_send_funds(user_request_send_funds);
                self
            },
            IncomingControlMessage::ReceiptAck(receipt_ack) => {
                self.control_receipt_ack(receipt_ack);
                self
            }
        };
        Ok(fself)
    }
}
