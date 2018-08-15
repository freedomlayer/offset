#![allow(unused)]
use ring::rand::SecureRandom;

use crypto::identity::PublicKey;

use super::super::token_channel::types::TcMutation;
use super::super::token_channel::directional::DirectionalMutation;
use super::super::slot::{SlotMutation, TokenChannelSlot, InconsistencyStatus,
                            IncomingInconsistency};
use super::super::friend::{FriendState, FriendMutation};
use super::super::state::{MessengerMutation, MessengerState};
use super::{MutableMessengerHandler, MessengerTask};
use app_manager::messages::{NetworkerCommand, AddFriend, 
    RemoveFriend, SetFriendStatus, SetFriendRemoteMaxDebt,
    ResetFriendChannel, SetFriendMaxChannels};


pub enum HandleAppManagerError {
    FriendDoesNotExist,
    TokenChannelDoesNotExist,
    NotInvitedToReset,
    ResetTokenMismatch,
}

#[allow(unused)]
impl<R: SecureRandom> MutableMessengerHandler<R> {

    fn get_friend(&self, friend_public_key: &PublicKey) -> Result<&FriendState, HandleAppManagerError> {
        match self.state().friends.get(friend_public_key) {
            Some(ref friend) => Ok(friend),
            None => Err(HandleAppManagerError::FriendDoesNotExist),
        }
    }

    fn get_slot(&self, friend_public_key: &PublicKey, channel_index: u16) -> Result<&TokenChannelSlot, HandleAppManagerError> {
        let friend = self.get_friend(&friend_public_key)?;
        match friend.tc_slots.get(&channel_index) {
            Some(ref tc_slot) => Ok(tc_slot),
            None => Err(HandleAppManagerError::TokenChannelDoesNotExist),
        }
    }

    fn app_manager_set_friend_remote_max_debt(&mut self, 
                                                set_friend_remote_max_debt: SetFriendRemoteMaxDebt) 
        -> Result<(), HandleAppManagerError> {

        // Make sure that friend exists:
        let _friend = self.get_friend(&set_friend_remote_max_debt.friend_public_key)?;

        let tc_mutation = TcMutation::SetRemoteMaxDebt(set_friend_remote_max_debt.remote_max_debt);
        let directional_mutation = DirectionalMutation::TcMutation(tc_mutation);
        let slot_mutation = SlotMutation::DirectionalMutation(directional_mutation);
        let friend_mutation = FriendMutation::SlotMutation(
            (set_friend_remote_max_debt.channel_index, slot_mutation));
        let m_mutation = MessengerMutation::FriendMutation(
            (set_friend_remote_max_debt.friend_public_key, friend_mutation));

        self.apply_mutation(m_mutation);
        Ok(())
    }

    fn app_manager_reset_friend_channel(&mut self, 
                                          reset_friend_channel: ResetFriendChannel) 
        -> Result<(), HandleAppManagerError> {

        let slot = self.get_slot(&reset_friend_channel.friend_public_key,
                                 reset_friend_channel.channel_index)?;

        match &slot.inconsistency_status.incoming {
            IncomingInconsistency::Empty => return Err(HandleAppManagerError::NotInvitedToReset),
            IncomingInconsistency::Incoming(reset_terms) => {
                if (reset_terms.current_token != reset_friend_channel.current_token)  {
                    return Err(HandleAppManagerError::ResetTokenMismatch);
                }
            }
        };


        let slot_mutation = SlotMutation::LocalReset;
        let friend_mutation = FriendMutation::SlotMutation(
            (reset_friend_channel.channel_index, slot_mutation));
        let m_mutation = MessengerMutation::FriendMutation(
            (reset_friend_channel.friend_public_key, friend_mutation));

        self.apply_mutation(m_mutation);
        Ok(())

    }

    fn app_manager_set_friend_max_channels(&mut self, 
                                          set_friend_max_channels: SetFriendMaxChannels) 
        -> Result<(), HandleAppManagerError> {

        // Make sure that friend exists:
        let _friend = self.get_friend(&set_friend_max_channels.friend_public_key)?;

        let friend_mutation = FriendMutation::SetLocalMaxChannels(set_friend_max_channels.max_channels);
        let m_mutation = MessengerMutation::FriendMutation(
            (set_friend_max_channels.friend_public_key, friend_mutation));

        self.apply_mutation(m_mutation);
        Ok(())
    }

    fn app_manager_add_friend(&mut self, 
                                add_friend: AddFriend) 
        -> Result<(), HandleAppManagerError> {

        let m_mutation = MessengerMutation::AddFriend((
                add_friend.friend_public_key,
                add_friend.friend_addr,
                add_friend.max_channels));

        self.apply_mutation(m_mutation);
        Ok(())
    }

    fn app_manager_remove_friend(&mut self, remove_friend: RemoveFriend) 
        -> Result<(), HandleAppManagerError> {

        let m_mutation = MessengerMutation::RemoveFriend(
                remove_friend.friend_public_key);

        self.apply_mutation(m_mutation);
        Ok(())
    }


    fn app_manager_set_friend_status(&mut self, set_friend_status: SetFriendStatus) 
        -> Result<(), HandleAppManagerError> {

        // Make sure that friend exists:
        let _friend = self.get_friend(&set_friend_status.friend_public_key)?;

        let friend_mutation = FriendMutation::SetStatus(set_friend_status.status);
        let m_mutation = MessengerMutation::FriendMutation(
            (set_friend_status.friend_public_key, friend_mutation));

        self.apply_mutation(m_mutation);
        Ok(())
    }

    pub fn handle_app_manager_message(&mut self, 
                                      networker_config: NetworkerCommand) 
        -> Result<(), HandleAppManagerError> {


        match networker_config {
            NetworkerCommand::SetFriendRemoteMaxDebt(set_friend_remote_max_debt) => 
                self.app_manager_set_friend_remote_max_debt(set_friend_remote_max_debt),
            NetworkerCommand::ResetFriendChannel(reset_friend_channel) => 
                self.app_manager_reset_friend_channel(reset_friend_channel),
            NetworkerCommand::SetFriendMaxChannels(set_friend_max_channels) => 
                self.app_manager_set_friend_max_channels(set_friend_max_channels),
            NetworkerCommand::AddFriend(add_friend) => 
                self.app_manager_add_friend(add_friend),
            NetworkerCommand::RemoveFriend(remove_friend) => 
                self.app_manager_remove_friend(remove_friend),
            NetworkerCommand::SetFriendStatus(set_friend_status) => 
                self.app_manager_set_friend_status(set_friend_status),
            NetworkerCommand::OpenFriendChannel(open_friend_channel) => unimplemented!(),
            NetworkerCommand::CloseFriendChannel(close_friend_channel) => unimplemented!(),
            NetworkerCommand::SetFriendAddr(set_friend_addr) => unimplemented!(),
            NetworkerCommand::SetFriendIncomingPathFee(set_friend_incoming_path_fee) => unimplemented!(),
        }
    }

}
