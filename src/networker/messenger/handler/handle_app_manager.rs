#![allow(unused)]
use ring::rand::SecureRandom;

use super::super::token_channel::types::TcMutation;
use super::super::token_channel::directional::DirectionalMutation;
use super::super::slot::SlotMutation;
use super::super::neighbor::{NeighborState, NeighborMutation};
use super::super::messenger_state::{MessengerMutation, MessengerState};
use super::{MessengerHandler, MessengerTask};
use app_manager::messages::{NetworkerCommand, AddNeighbor, 
    RemoveNeighbor, SetNeighborStatus, SetNeighborRemoteMaxDebt,
    ResetNeighborChannel, SetNeighborMaxChannels};


/*
pub enum HandleAppManagerError {
    NeighborDoesNotExist,
    TokenChannelDoesNotExist,
    NeighborAlreadyExists,
    MessengerStateError(MessengerStateError),
}
*/

#[allow(unused)]
impl<R: SecureRandom> MessengerHandler<R> {
    fn app_manager_set_neighbor_remote_max_debt(&self, 
                                                set_neighbor_remote_max_debt: SetNeighborRemoteMaxDebt) 
        -> (Vec<MessengerMutation>, Vec<MessengerTask>) {

        let tc_mutation = TcMutation::SetRemoteMaxDebt(set_neighbor_remote_max_debt.remote_max_debt);
        let directional_mutation = DirectionalMutation::TcMutation(tc_mutation);
        let slot_mutation = SlotMutation::DirectionalMutation(directional_mutation);
        let neighbor_mutation = NeighborMutation::SlotMutation(
            (set_neighbor_remote_max_debt.channel_index, slot_mutation));
        let m_mutation = MessengerMutation::NeighborMutation(
            (set_neighbor_remote_max_debt.neighbor_public_key, neighbor_mutation));

        (vec![m_mutation], vec![])
    }



/*
#[derive(Clone)]
pub struct ResetNeighborChannel {
    pub neighbor_public_key: PublicKey,
    pub channel_index: u16,
    pub current_token: ChannelToken,
    pub balance_for_reset: i64,
}
*/

    fn app_manager_reset_neighbor_channel(&mut self, 
                                          reset_neighbor_channel: ResetNeighborChannel) 
        -> (Vec<MessengerMutation>, Vec<MessengerTask>) {

        // TODO: Maybe allow smaller interface to the ability of resetting a neighbor's channel?
        let directional_mutation = DirectionalMutation::Reset((
                reset_neighbor_channel.current_token,
                reset_neighbor_channel.balance_for_reset));
        let slot_mutation = SlotMutation::DirectionalMutation(directional_mutation);
        let neighbor_mutation = NeighborMutation::SlotMutation(
            (reset_neighbor_channel.channel_index, slot_mutation));
        let m_mutation = MessengerMutation::NeighborMutation(
            (reset_neighbor_channel.neighbor_public_key, neighbor_mutation));
        (vec![m_mutation], vec![])

    }

    fn app_manager_set_neighbor_max_channels(&mut self, 
                                          set_neighbor_max_channels: SetNeighborMaxChannels) 
        -> (Vec<MessengerMutation>, Vec<MessengerTask>) {

        let neighbor_mutation = NeighborMutation::SetLocalMaxChannels(set_neighbor_max_channels.max_channels);
        let m_mutation = MessengerMutation::NeighborMutation(
            (set_neighbor_max_channels.neighbor_public_key, neighbor_mutation));

        (vec![m_mutation], vec![])
    }

    fn app_manager_add_neighbor(&mut self, 
                                add_neighbor: AddNeighbor) 
        -> (Vec<MessengerMutation>, Vec<MessengerTask>) {

        let m_mutation = MessengerMutation::AddNeighbor((
                add_neighbor.neighbor_public_key,
                add_neighbor.neighbor_addr,
                add_neighbor.max_channels));

        (vec![m_mutation], vec![])
    }

    fn app_manager_remove_neighbor(&mut self, remove_neighbor: RemoveNeighbor) 
        -> (Vec<MessengerMutation>, Vec<MessengerTask>) {

        let m_mutation = MessengerMutation::RemoveNeighbor(
                remove_neighbor.neighbor_public_key);

        (vec![m_mutation], vec![])
    }


    fn app_manager_set_neighbor_status(&mut self, set_neighbor_status: SetNeighborStatus) 
        -> (Vec<MessengerMutation>, Vec<MessengerTask>) {

        let neighbor_mutation = NeighborMutation::SetStatus(set_neighbor_status.status);
        let m_mutation = MessengerMutation::NeighborMutation(
            (set_neighbor_status.neighbor_public_key, neighbor_mutation));

        (vec![m_mutation], vec![])
    }

    pub fn handle_app_manager_message(&mut self, 
                                      networker_config: NetworkerCommand) 
        -> (Vec<MessengerMutation>, Vec<MessengerTask>) {


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
