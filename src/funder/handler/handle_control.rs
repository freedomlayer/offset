#![allow(unused)]
use ring::rand::SecureRandom;

use crypto::identity::PublicKey;

use super::super::token_channel::types::TcMutation;
use super::super::token_channel::directional::DirectionalMutation;
use super::super::friend::{FriendState, FriendMutation, IncomingInconsistency};
use super::super::state::{FunderMutation, FunderState};
use super::{MutableFunderHandler, FunderTask};
use super::super::messages::{FunderCommand, AddFriend, 
    RemoveFriend, SetFriendStatus, SetFriendRemoteMaxDebt,
    ResetFriendChannel, SetRequestsStatus, SetFriendAddr, RequestSendFundsMsg};


pub enum HandleControlError {
    FriendDoesNotExist,
    TokenChannelDoesNotExist,
    NotInvitedToReset,
    ResetTokenMismatch,
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

    fn control_request_send_funds_msg(&mut self, request_send_funds_msg: RequestSendFundsMsg) 
        -> Result<(), HandleControlError> {

        // TODO
        unimplemented!();
    }


    pub fn handle_control_message(&mut self, 
                                      funder_config: FunderCommand<A>) 
        -> Result<(), HandleControlError> {


        match funder_config {
            FunderCommand::SetFriendRemoteMaxDebt(set_friend_remote_max_debt) => 
                self.control_set_friend_remote_max_debt(set_friend_remote_max_debt),
            FunderCommand::ResetFriendChannel(reset_friend_channel) => 
                self.control_reset_friend_channel(reset_friend_channel),
            FunderCommand::AddFriend(add_friend) => 
                self.control_add_friend(add_friend),
            FunderCommand::RemoveFriend(remove_friend) => 
                self.control_remove_friend(remove_friend),
            FunderCommand::SetFriendStatus(set_friend_status) => 
                self.control_set_friend_status(set_friend_status),
            FunderCommand::SetRequestsStatus(set_requests_status) => 
                self.control_set_requests_status(set_requests_status),
            FunderCommand::SetFriendAddr(set_friend_addr) => 
                self.control_set_friend_addr(set_friend_addr),
            FunderCommand::RequestSendFundsMsg(request_send_funds_msg) => 
                self.control_request_send_funds_msg(request_send_funds_msg),
        }
    }
}
