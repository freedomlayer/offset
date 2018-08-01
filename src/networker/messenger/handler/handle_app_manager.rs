#![allow(unused)]
use ring::rand::SecureRandom;

use crypto::identity::PublicKey;

use super::super::token_channel::types::TcMutation;
use super::super::token_channel::directional::DirectionalMutation;
use super::super::slot::{SlotMutation, TokenChannelSlot, TokenChannelStatus};
use super::super::neighbor::{NeighborState, NeighborMutation};
use super::super::messenger_state::{MessengerMutation, MessengerState};
use super::{MutableMessengerHandler, MessengerTask};
use app_manager::messages::{NetworkerCommand, AddNeighbor, 
    RemoveNeighbor, SetNeighborStatus, SetNeighborRemoteMaxDebt,
    ResetNeighborChannel, SetNeighborMaxChannels};


pub enum HandleAppManagerError {
    NeighborDoesNotExist,
    TokenChannelDoesNotExist,
    NotInvitedToReset,
    ResetTokenMismatch,
}

#[allow(unused)]
impl<R: SecureRandom> MutableMessengerHandler<R> {

    fn get_neighbor(&self, neighbor_public_key: &PublicKey) -> Result<&NeighborState, HandleAppManagerError> {
        match self.state().neighbors.get(neighbor_public_key) {
            Some(ref neighbor) => Ok(neighbor),
            None => Err(HandleAppManagerError::NeighborDoesNotExist),
        }
    }

    fn get_slot(&self, neighbor_public_key: &PublicKey, channel_index: u16) -> Result<&TokenChannelSlot, HandleAppManagerError> {
        let neighbor = self.get_neighbor(&neighbor_public_key)?;
        match neighbor.tc_slots.get(&channel_index) {
            Some(ref tc_slot) => Ok(tc_slot),
            None => Err(HandleAppManagerError::TokenChannelDoesNotExist),
        }
    }

    fn app_manager_set_neighbor_remote_max_debt(&mut self, 
                                                set_neighbor_remote_max_debt: SetNeighborRemoteMaxDebt) 
        -> Result<(), HandleAppManagerError> {

        // Make sure that neighbor exists:
        let _neighbor = self.get_neighbor(&set_neighbor_remote_max_debt.neighbor_public_key)?;

        let tc_mutation = TcMutation::SetRemoteMaxDebt(set_neighbor_remote_max_debt.remote_max_debt);
        let directional_mutation = DirectionalMutation::TcMutation(tc_mutation);
        let slot_mutation = SlotMutation::DirectionalMutation(directional_mutation);
        let neighbor_mutation = NeighborMutation::SlotMutation(
            (set_neighbor_remote_max_debt.channel_index, slot_mutation));
        let m_mutation = MessengerMutation::NeighborMutation(
            (set_neighbor_remote_max_debt.neighbor_public_key, neighbor_mutation));

        self.apply_mutation(m_mutation);
        Ok(())
    }

    fn app_manager_reset_neighbor_channel(&mut self, 
                                          reset_neighbor_channel: ResetNeighborChannel) 
        -> Result<(), HandleAppManagerError> {

        let slot = self.get_slot(&reset_neighbor_channel.neighbor_public_key,
                                 reset_neighbor_channel.channel_index)?;

        match &slot.tc_status {
            TokenChannelStatus::Valid => return Err(HandleAppManagerError::NotInvitedToReset),
            TokenChannelStatus::Inconsistent { current_token, .. } => {
                if (current_token != &reset_neighbor_channel.current_token)  {
                    return Err(HandleAppManagerError::ResetTokenMismatch);
                }
            }
        };


        let slot_mutation = SlotMutation::LocalReset;
        let neighbor_mutation = NeighborMutation::SlotMutation(
            (reset_neighbor_channel.channel_index, slot_mutation));
        let m_mutation = MessengerMutation::NeighborMutation(
            (reset_neighbor_channel.neighbor_public_key, neighbor_mutation));

        self.apply_mutation(m_mutation);
        Ok(())

    }

    fn app_manager_set_neighbor_max_channels(&mut self, 
                                          set_neighbor_max_channels: SetNeighborMaxChannels) 
        -> Result<(), HandleAppManagerError> {

        // Make sure that neighbor exists:
        let _neighbor = self.get_neighbor(&set_neighbor_max_channels.neighbor_public_key)?;

        let neighbor_mutation = NeighborMutation::SetLocalMaxChannels(set_neighbor_max_channels.max_channels);
        let m_mutation = MessengerMutation::NeighborMutation(
            (set_neighbor_max_channels.neighbor_public_key, neighbor_mutation));

        self.apply_mutation(m_mutation);
        Ok(())
    }

    fn app_manager_add_neighbor(&mut self, 
                                add_neighbor: AddNeighbor) 
        -> Result<(), HandleAppManagerError> {

        let m_mutation = MessengerMutation::AddNeighbor((
                add_neighbor.neighbor_public_key,
                add_neighbor.neighbor_addr,
                add_neighbor.max_channels));

        self.apply_mutation(m_mutation);
        Ok(())
    }

    fn app_manager_remove_neighbor(&mut self, remove_neighbor: RemoveNeighbor) 
        -> Result<(), HandleAppManagerError> {

        let m_mutation = MessengerMutation::RemoveNeighbor(
                remove_neighbor.neighbor_public_key);

        self.apply_mutation(m_mutation);
        Ok(())
    }


    fn app_manager_set_neighbor_status(&mut self, set_neighbor_status: SetNeighborStatus) 
        -> Result<(), HandleAppManagerError> {

        // Make sure that neighbor exists:
        let _neighbor = self.get_neighbor(&set_neighbor_status.neighbor_public_key)?;

        let neighbor_mutation = NeighborMutation::SetStatus(set_neighbor_status.status);
        let m_mutation = MessengerMutation::NeighborMutation(
            (set_neighbor_status.neighbor_public_key, neighbor_mutation));

        self.apply_mutation(m_mutation);
        Ok(())
    }

    pub fn handle_app_manager_message(&mut self, 
                                      networker_config: NetworkerCommand) 
        -> Result<(), HandleAppManagerError> {


        match networker_config {
            NetworkerCommand::SetNeighborRemoteMaxDebt(set_neighbor_remote_max_debt) => 
                self.app_manager_set_neighbor_remote_max_debt(set_neighbor_remote_max_debt),
            NetworkerCommand::ResetNeighborChannel(reset_neighbor_channel) => 
                self.app_manager_reset_neighbor_channel(reset_neighbor_channel),
            NetworkerCommand::SetNeighborMaxChannels(set_neighbor_max_channels) => 
                self.app_manager_set_neighbor_max_channels(set_neighbor_max_channels),
            NetworkerCommand::AddNeighbor(add_neighbor) => 
                self.app_manager_add_neighbor(add_neighbor),
            NetworkerCommand::RemoveNeighbor(remove_neighbor) => 
                self.app_manager_remove_neighbor(remove_neighbor),
            NetworkerCommand::SetNeighborStatus(set_neighbor_status) => 
                self.app_manager_set_neighbor_status(set_neighbor_status),
            NetworkerCommand::OpenNeighborChannel(open_neighbor_channel) => unimplemented!(),
            NetworkerCommand::CloseNeighborChannel(close_neighbor_channel) => unimplemented!(),
            NetworkerCommand::SetNeighborAddr(set_neighbor_addr) => unimplemented!(),
            NetworkerCommand::SetNeighborIncomingPathFee(set_neighbor_incoming_path_fee) => unimplemented!(),
        }
    }

}
