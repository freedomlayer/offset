#![allow(unused)]
use ring::rand::SecureRandom;

use crypto::identity::PublicKey;
use crypto::uid::Uid;
use crypto::hash::HashResult;
use proto::funder::{ChannelToken, InvoiceId};

use super::super::token_channel::types::TcMutation;
use super::super::token_channel::directional::DirectionalMutation;
use super::super::friend::{FriendState, FriendMutation, IncomingInconsistency};
use super::super::state::{FunderMutation, FunderState};
use super::{MutableFunderHandler, FunderTask};
use super::super::types::{FriendStatus, RequestsStatus, 
    FriendsRoute, UserRequestSendFunds};
use super::ResponseReceived;
use super::super::messages::ResponseSendFundsResult;

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
}

pub struct SetFriendRemoteMaxDebt {
    pub friend_public_key: PublicKey,
    pub remote_max_debt: u128,
}

pub struct ResetFriendChannel {
    pub friend_public_key: PublicKey,
    pub current_token: ChannelToken,
}

pub struct SetFriendAddr<A> {
    pub friend_public_key: PublicKey,
    pub address: Option<A>,
}

pub struct AddFriend<A> {
    pub friend_public_key: PublicKey,
    pub address: Option<A>,
}

pub struct RemoveFriend {
    pub friend_public_key: PublicKey,
}

pub struct SetFriendStatus {
    pub friend_public_key: PublicKey,
    pub status: FriendStatus,
}

pub struct SetRequestsStatus {
    pub friend_public_key: PublicKey,
    pub status: RequestsStatus,
}


pub struct ReceiptAck {
    pub request_id: Uid,
    pub receipt_hash: HashResult,
}

pub enum IncomingControlMessage<A> {
    AddFriend(AddFriend<A>),
    RemoveFriend(RemoveFriend),
    SetRequestsStatus(SetRequestsStatus),
    SetFriendStatus(SetFriendStatus),
    SetFriendRemoteMaxDebt(SetFriendRemoteMaxDebt),
    SetFriendAddr(SetFriendAddr<A>),
    ResetFriendChannel(ResetFriendChannel),
    RequestSendFunds(UserRequestSendFunds),
    ReceiptAck(ReceiptAck),
}


#[allow(unused)]
impl<A:Clone ,R: SecureRandom> MutableFunderHandler<A,R> {

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

    fn control_reset_friend_channel(&mut self, 
                                          reset_friend_channel: ResetFriendChannel) 
        -> Result<(), HandleControlError> {

        let friend = self.get_friend(&reset_friend_channel.friend_public_key)
            .ok_or(HandleControlError::FriendDoesNotExist)?;

        match &friend.inconsistency_status.incoming {
            IncomingInconsistency::Empty => return Err(HandleControlError::NotInvitedToReset),
            IncomingInconsistency::Incoming(reset_terms) => {
                if (reset_terms.current_token != reset_friend_channel.current_token)  {
                    return Err(HandleControlError::ResetTokenMismatch);
                }
            }
        };

        let friend_mutation = FriendMutation::LocalReset;
        let m_mutation = FunderMutation::FriendMutation(
            (reset_friend_channel.friend_public_key, friend_mutation));

        self.apply_mutation(m_mutation);
        Ok(())

    }

    fn control_add_friend(&mut self, add_friend: AddFriend<A>) 
        -> Result<(), HandleControlError> {

        let m_mutation = FunderMutation::AddFriend((
                add_friend.friend_public_key,
                add_friend.address));

        self.apply_mutation(m_mutation);
        Ok(())
    }

    fn control_remove_friend(&mut self, remove_friend: RemoveFriend) 
        -> Result<(), HandleControlError> {

        let m_mutation = FunderMutation::RemoveFriend(
                remove_friend.friend_public_key);

        self.apply_mutation(m_mutation);
        Ok(())
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

    fn control_request_send_funds_inner(&mut self, user_request_send_funds: UserRequestSendFunds)
        -> Result<(), HandleControlError> {

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

        // TODO: Trigger a function that tries to send stuff to the remote side.
        
        
        // - Check if we have room to push this message.
        //   If we don't, we return an error.
        //
        // - Push the message to the queue.
        // - If we have the token: Trigger a function that tries to send stuff to the remote side 
        //      - If the token is at our side, we might be able to send the request immediately.
        //      - If the token is at the remote side, we should signal the remote side to give us
        //      the token.
        
        unimplemented!();

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

    fn control_receipt_ack(&mut self, receipt_ack: ReceiptAck) 
        -> Result<(), HandleControlError> {

        //  TODO: Remove the receipt from self.state.ready_receipts.
        // TODO
        unimplemented!();
    }


    pub fn handle_control_message(&mut self, 
                                  funder_config: IncomingControlMessage<A>) 
        -> Result<(), HandleControlError> {


        match funder_config {
            IncomingControlMessage::SetFriendRemoteMaxDebt(set_friend_remote_max_debt) => 
                self.control_set_friend_remote_max_debt(set_friend_remote_max_debt),
            IncomingControlMessage::ResetFriendChannel(reset_friend_channel) => 
                self.control_reset_friend_channel(reset_friend_channel),
            IncomingControlMessage::AddFriend(add_friend) => 
                self.control_add_friend(add_friend),
            IncomingControlMessage::RemoveFriend(remove_friend) => 
                self.control_remove_friend(remove_friend),
            IncomingControlMessage::SetFriendStatus(set_friend_status) => 
                self.control_set_friend_status(set_friend_status),
            IncomingControlMessage::SetRequestsStatus(set_requests_status) => 
                self.control_set_requests_status(set_requests_status),
            IncomingControlMessage::SetFriendAddr(set_friend_addr) => 
                self.control_set_friend_addr(set_friend_addr),
            IncomingControlMessage::RequestSendFunds(user_request_send_funds) => 
                self.control_request_send_funds(user_request_send_funds),
            IncomingControlMessage::ReceiptAck(receipt_ack) => 
                self.control_receipt_ack(receipt_ack),
        }
    }
}
