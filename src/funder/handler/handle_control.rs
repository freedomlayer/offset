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
use super::super::types::{FriendStatus, RequestsStatus, FriendsRoute};


pub enum HandleControlError {
    FriendDoesNotExist,
    TokenChannelDoesNotExist,
    NotInvitedToReset,
    ResetTokenMismatch,
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

pub struct RequestSendFunds {
    pub request_id: Uid,
    pub route: FriendsRoute,
    pub invoice_id: InvoiceId,
    pub payment: u128,
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
    RequestSendFunds(RequestSendFunds),
    ReceiptAck(ReceiptAck),
}


#[allow(unused)]
impl<A:Clone ,R: SecureRandom> MutableFunderHandler<A,R> {
    fn get_friend_control(&self, friend_public_key: &PublicKey) -> Result<&FriendState<A>, HandleControlError> {
        match self.state().friends.get(friend_public_key) {
            Some(ref friend) => Ok(friend),
            None => Err(HandleControlError::FriendDoesNotExist),
        }
    }

    fn control_set_friend_remote_max_debt(&mut self, 
                                                set_friend_remote_max_debt: SetFriendRemoteMaxDebt) 
        -> Result<(), HandleControlError> {

        // Make sure that friend exists:
        let _friend = self.get_friend_control(&set_friend_remote_max_debt.friend_public_key)?;

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

        let friend = self.get_friend_control(&reset_friend_channel.friend_public_key)?;

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
        let _friend = self.get_friend_control(&set_friend_status.friend_public_key)?;

        let friend_mutation = FriendMutation::SetStatus(set_friend_status.status);
        let m_mutation = FunderMutation::FriendMutation(
            (set_friend_status.friend_public_key, friend_mutation));

        self.apply_mutation(m_mutation);
        Ok(())
    }

    fn control_set_requests_status(&mut self, set_requests_status: SetRequestsStatus) 
        -> Result<(), HandleControlError> {

        // Make sure that friend exists:
        let _friend = self.get_friend_control(&set_requests_status.friend_public_key)?;

        let friend_mutation = FriendMutation::SetWantedLocalRequestsStatus(set_requests_status.status);
        let m_mutation = FunderMutation::FriendMutation(
            (set_requests_status.friend_public_key, friend_mutation));

        self.apply_mutation(m_mutation);
        Ok(())
    }

    fn control_set_friend_addr(&mut self, set_friend_addr: SetFriendAddr<A>) 
        -> Result<(), HandleControlError> {

        // Make sure that friend exists:
        let _friend = self.get_friend_control(&set_friend_addr.friend_public_key)?;

        let friend_mutation = FriendMutation::SetFriendAddr(set_friend_addr.address);
        let m_mutation = FunderMutation::FriendMutation(
            (set_friend_addr.friend_public_key, friend_mutation));

        self.apply_mutation(m_mutation);
        Ok(())
    }

    fn control_request_send_funds(&mut self, request_send_funds: RequestSendFunds) 
        -> Result<(), HandleControlError> {

        // TODO:
        // Attempt to push message to pending requests queue of a friend.
        // This queue should not be the same as the pending operations queue.
        //
        // If there is room, the message will wait in the queue for a time when the token is at our
        // side. If there is no room, a failure message will be returned, having the local public
        // key as the failure originator.

        // TODO
        unimplemented!();
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
            IncomingControlMessage::RequestSendFunds(request_send_funds) => 
                self.control_request_send_funds(request_send_funds),
            IncomingControlMessage::ReceiptAck(receipt_ack) => 
                self.control_receipt_ack(receipt_ack),
        }
    }
}
