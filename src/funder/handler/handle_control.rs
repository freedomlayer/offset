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


pub enum HandleControlError {
    FriendDoesNotExist,
    TokenChannelDoesNotExist,
    NotInvitedToReset,
    ResetTokenMismatch,
    NotFirstInRoute,
    InvalidRoute,
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

        // TODO:
        // Attempt to push message to pending requests queue of a friend.
        // This queue should not be the same as the pending operations queue.
        //
        // If there is room, the message will wait in the queue for a time when the token is at our
        // side. If there is no room, a failure message will be returned, having the local public
        // key as the failure originator.
        //
        // Should we check if a mesasge with the same request_id is already in progress?

        // Check if we have room in the pending queue:
        let route = &user_request_send_funds.route;

        // We have to be the first on the route:
        match route.public_keys.first() {
            Some(first) if *first == self.state.local_public_key => Ok(()),
            // TODO: Possibly return here a failure message?
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

        // TODO:
        // - Check if there is a message with identical request in progress somewhere:
        //      - Inside the pending queue.
        //      - As a pending request.
        //  If so, we return success and don't push anything.
        //
        // - Check if we have room to push this message.
        //   If we don't, we return an error.
        
        unimplemented!();
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
