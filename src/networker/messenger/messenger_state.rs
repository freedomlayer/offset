use im::hashmap::HashMap as ImHashMap;


use num_bigint::BigUint;
use num_traits::identities::Zero;

use crypto::identity::PublicKey;
// use crypto::rand_values::RandValue;

use proto::networker::ChannelToken;

use super::types::{NeighborTcOp, NeighborMoveToken, RequestSendMessage};

use super::slot::TokenChannelSlot;
use super::token_channel::directional::{ReceiveMoveTokenOutput, ReceiveMoveTokenError};
use super::token_channel::types::NeighborMoveTokenInner;
use super::neighbor::NeighborState;

use app_manager::messages::{SetNeighborRemoteMaxDebt, SetNeighborMaxChannels, 
    AddNeighbor, RemoveNeighbor, ResetNeighborChannel, SetNeighborStatus};



#[allow(unused)]
pub struct MessengerState {
    local_public_key: PublicKey,
    neighbors: ImHashMap<PublicKey, NeighborState>,
}

#[allow(unused)]
#[derive(Clone)]
pub struct SmInitTokenChannel {
    pub neighbor_public_key: PublicKey,
    pub channel_index: u16,
}

#[allow(unused)]
#[derive(Clone)]
pub struct SmTokenChannelPushOp {
    pub neighbor_public_key: PublicKey, 
    pub channel_index: u16, 
    pub neighbor_op: NeighborTcOp
}

#[derive(Clone)]
pub struct SmNeighborPushRequest {
    pub neighbor_public_key: PublicKey,
    pub request: RequestSendMessage,
}

#[allow(unused)]
#[derive(Clone)]
pub struct SmResetTokenChannel {
    pub neighbor_public_key: PublicKey, 
    pub channel_index: u16, 
    pub reset_token: ChannelToken,
    pub balance_for_reset: i64,
}

#[allow(unused)]
#[derive(Clone)]
// TODO: Possibly change name to SmIncomingNeighborMoveToken.
pub struct SmApplyNeighborMoveToken {
    pub neighbor_public_key: PublicKey, 
    pub neighbor_move_token: NeighborMoveToken,
}

#[allow(unused)]
#[derive(Clone)]
pub struct SmOutgoingNeighborMoveToken {
    pub neighbor_public_key: PublicKey, 
    pub neighbor_move_token: NeighborMoveToken,
}


#[allow(unused)]
#[derive(Clone)]
pub enum StateMutateMessage {
    SetNeighborRemoteMaxDebt(SetNeighborRemoteMaxDebt),
    SetNeighborMaxChannels(SetNeighborMaxChannels),
    ResetNeighborChannel(ResetNeighborChannel),
    AddNeighbor(AddNeighbor),
    RemoveNeighbor(RemoveNeighbor),
    SetNeighborStatus(SetNeighborStatus),
    InitTokenChannel(SmInitTokenChannel),
    TokenChannelPushOp(SmTokenChannelPushOp),
    NeighborPushRequest(SmNeighborPushRequest),
    ResetTokenChannel(SmResetTokenChannel),
    ApplyNeighborMoveToken(SmApplyNeighborMoveToken),
    OutgoingNeighborMoveToken(SmOutgoingNeighborMoveToken),
}


#[allow(unused)]
#[derive(Debug)]
pub enum MessengerStateError {
    NeighborDoesNotExist,
    TokenChannelDoesNotExist,
    NeighborAlreadyExists,
    TokenChannelAlreadyExists,
    ReceiveMoveTokenError(ReceiveMoveTokenError),
    TokenChannelAlreadyOutgoing,
}

#[allow(unused)]
impl MessengerState {
    pub fn new() -> MessengerState {
        // TODO: Initialize from database somehow.
        unreachable!();
    }

    /// Get total trust (in credits) we put on all the neighbors together.
    pub fn get_total_trust(&self) -> BigUint {
        let mut sum: BigUint = BigUint::zero();
        for neighbor in self.neighbors.values() {
            sum += neighbor.get_trust();
        }
        sum
    }

    pub fn get_neighbors(&self) -> &ImHashMap<PublicKey, NeighborState> {
        &self.neighbors
    }

    pub fn get_local_public_key(&self) -> &PublicKey {
        &self.local_public_key
    }

    pub fn set_neighbor_remote_max_debt(&mut self, 
                                        set_neighbor_remote_max_debt: SetNeighborRemoteMaxDebt)
                                        -> Result<(), MessengerStateError> {

        let neighbor_state = self.neighbors.get_mut(&set_neighbor_remote_max_debt.neighbor_public_key)
            .ok_or(MessengerStateError::NeighborDoesNotExist)?;
        
        // Find the token channel slot:
        let token_channel_slot = neighbor_state.token_channel_slots
            .get_mut(&set_neighbor_remote_max_debt.channel_index)
            .ok_or(MessengerStateError::TokenChannelDoesNotExist)?;

        token_channel_slot.wanted_remote_max_debt = set_neighbor_remote_max_debt.remote_max_debt;
        Ok(())
    }


    // TODO: This method is very similar to reset_token_channel. Could/Should we unite them?
    pub fn reset_neighbor_channel(&mut self, 
                                    reset_neighbor_channel: ResetNeighborChannel) 
                                    -> Result<(), MessengerStateError> {
                                        
        let neighbor_state = self.neighbors.get_mut(&reset_neighbor_channel.neighbor_public_key)
            .ok_or(MessengerStateError::NeighborDoesNotExist)?;

        let new_token_channel_slot = TokenChannelSlot::new_from_reset(
            &self.local_public_key,
            &reset_neighbor_channel.neighbor_public_key,
            reset_neighbor_channel.channel_index,
            &reset_neighbor_channel.current_token,
            reset_neighbor_channel.balance_for_reset);

        // Replace the old token channel slot with the new one:
        if !neighbor_state.token_channel_slots.contains_key(&reset_neighbor_channel.channel_index) {
            return Err(MessengerStateError::TokenChannelDoesNotExist);
        }
        
        neighbor_state.token_channel_slots.insert(reset_neighbor_channel.channel_index, 
                                                  new_token_channel_slot);

        Ok(())
    }

    pub fn set_neighbor_max_channels(&mut self, 
                                    set_neighbor_max_channels: SetNeighborMaxChannels) 
                                    -> Result<(), MessengerStateError> {

        // Check if we have the requested neighbor:
        let neighbor_state = self.neighbors.get_mut(&set_neighbor_max_channels.neighbor_public_key)
            .ok_or(MessengerStateError::NeighborDoesNotExist)?;

        neighbor_state.local_max_channels = set_neighbor_max_channels.max_channels;

        Ok(())
    }

    pub fn add_neighbor(&mut self, 
                        add_neighbor: AddNeighbor) 
                        -> Result<(), MessengerStateError> {

        // If we already have the neighbor: return error.
        if self.neighbors.contains_key(&add_neighbor.neighbor_public_key) {
            return Err(MessengerStateError::NeighborAlreadyExists);
        }

        // Otherwise, we add a new neighbor:
        let neighbor_state = NeighborState::new(
                add_neighbor.neighbor_addr,
                add_neighbor.max_channels);

        self.neighbors.insert(add_neighbor.neighbor_public_key.clone(), neighbor_state);

        Ok(())
    }

    fn get_neighbor(&self, neighbor_public_key: &PublicKey) 
        -> Result<&NeighborState, MessengerStateError> {

        self.neighbors.get(neighbor_public_key)
            .ok_or(MessengerStateError::NeighborDoesNotExist)
    }

    pub fn remove_neighbor(&mut self, 
                        remove_neighbor: RemoveNeighbor) 
                        -> Result<(), MessengerStateError> {

        let _ = self.neighbors.remove(&remove_neighbor.neighbor_public_key)
            .ok_or(MessengerStateError::NeighborDoesNotExist)?;

        Ok(())
    }

    pub fn set_neighbor_status(&mut self, 
                        set_neighbor_status: SetNeighborStatus) 
                        -> Result<(), MessengerStateError> {

        // Check if we have the requested neighbor:
        let neighbor_state = self.neighbors.get_mut(&set_neighbor_status.neighbor_public_key)
            .ok_or(MessengerStateError::NeighborDoesNotExist)?;

        neighbor_state.status = set_neighbor_status.status;

        Ok(())
    }


    pub fn init_token_channel(&mut self, init_token_channel: SmInitTokenChannel)
        -> Result<(), MessengerStateError> {

        if self.get_neighbor(&init_token_channel.neighbor_public_key)?
            .token_channel_slots.contains_key(&init_token_channel.channel_index) {
            return Err(MessengerStateError::TokenChannelAlreadyExists);
        }

        let neighbor = self.neighbors.get_mut(&init_token_channel.neighbor_public_key)
            .ok_or(MessengerStateError::NeighborDoesNotExist)?;

        neighbor.token_channel_slots.insert(
            init_token_channel.channel_index,
            TokenChannelSlot::new(&self.local_public_key,
                                     &init_token_channel.neighbor_public_key,
                                     init_token_channel.channel_index));
        Ok(())
    }

    pub fn token_channel_push_op(&mut self, 
                                 token_channel_push_op: SmTokenChannelPushOp) 
        -> Result<(), MessengerStateError> {

        let neighbor = self.neighbors.get_mut(&token_channel_push_op.neighbor_public_key)
            .ok_or(MessengerStateError::NeighborDoesNotExist)?;

        let token_channel_slot = neighbor.token_channel_slots
            .get_mut(&token_channel_push_op.channel_index)
            .ok_or(MessengerStateError::TokenChannelDoesNotExist)?;

        token_channel_slot.pending_operations.push_back(token_channel_push_op.neighbor_op.clone());

        Ok(())
    }

    pub fn neighbor_push_request(&mut self, 
                                 neighbor_push_request: SmNeighborPushRequest) 
        -> Result<(), MessengerStateError> {

        let neighbor = self.neighbors.get_mut(&neighbor_push_request.neighbor_public_key)
            .ok_or(MessengerStateError::NeighborDoesNotExist)?;

        neighbor.pending_requests.push_back(neighbor_push_request.request);

        Ok(())
    }

    pub fn reset_token_channel(&mut self, 
                                 reset_token_channel: SmResetTokenChannel) 
        -> Result<(), MessengerStateError> {

        let neighbor = self.neighbors.get_mut(&reset_token_channel.neighbor_public_key)
            .ok_or(MessengerStateError::NeighborDoesNotExist)?;

        let token_channel_slot = TokenChannelSlot::new_from_reset(
            &self.local_public_key,
            &reset_token_channel.neighbor_public_key,
            reset_token_channel.channel_index,
            &reset_token_channel.reset_token,
            reset_token_channel.balance_for_reset);

        let _ = neighbor.token_channel_slots.insert(
            reset_token_channel.channel_index, 
            token_channel_slot);

        Ok(())
    }

    pub fn apply_neighbor_move_token(&mut self, 
                                 apply_neighbor_move_token: SmApplyNeighborMoveToken) 
        -> Result<ReceiveMoveTokenOutput, MessengerStateError> {


        let neighbor = self.neighbors.get_mut(&apply_neighbor_move_token.neighbor_public_key)
            .ok_or(MessengerStateError::NeighborDoesNotExist)?;

        let channel_index = apply_neighbor_move_token
            .neighbor_move_token
            .token_channel_index;

        let token_channel_slot = neighbor.token_channel_slots
            .get_mut(&channel_index)
            .ok_or(MessengerStateError::TokenChannelDoesNotExist)?;

        let NeighborMoveToken { operations, old_token, rand_nonce, .. } = 
            apply_neighbor_move_token.neighbor_move_token;

        let inner_move_token = NeighborMoveTokenInner {
            operations,
            old_token,
            rand_nonce,
        };

        let new_token = apply_neighbor_move_token.neighbor_move_token.new_token;

        token_channel_slot.directional.simulate_receive_move_token(inner_move_token, new_token)
            .map_err(MessengerStateError::ReceiveMoveTokenError)

    }

}
