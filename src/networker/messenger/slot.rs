use im::vector::Vector;

use crypto::identity::PublicKey;
use crypto::uid::Uid;

use super::token_channel::directional::DirectionalMutation;
use proto::networker::{ChannelToken, NetworkerSendPrice};
use super::types::{NeighborTcOp};
use super::token_channel::directional::{DirectionalTokenChannel};


#[allow(dead_code)]
#[derive(Clone)]
pub enum TokenChannelStatus {
    Valid,
    /// Inconsistent means that the remote side showed disagreement about the 
    /// token channel, and this channel is waiting for a local human intervention.
    Inconsistent {
        current_token: ChannelToken,
        balance_for_reset: i64,
    },
}

#[allow(unused)]
pub enum SlotMutation {
    DirectionalMutation(DirectionalMutation),
    SetTcStatus(TokenChannelStatus),
    SetWantedRemoteMaxDebt(u64),
    SetWantedLocalSendPrice(Option<NetworkerSendPrice>),
    PushBackPendingOperation(NeighborTcOp),
    PopFrontPendingOperation,
    SetPendingSendFundsId(Uid),
    ClearPendingSendFundsId,
    RemoteReset,        // Remote side performed reset
    LocalReset,         // Local side performed reset
}

#[allow(unused)]
#[derive(Clone)]
pub struct TokenChannelSlot {
    pub directional: DirectionalTokenChannel,
    pub tc_status: TokenChannelStatus,
    pub wanted_remote_max_debt: u64,
    pub wanted_local_send_price: Option<NetworkerSendPrice>,
    pub pending_operations: Vector<NeighborTcOp>,
    // Pending operations to be sent to the token channel.
    pending_send_funds_id: Option<Uid>,
}


#[allow(unused)]
impl TokenChannelSlot {
    pub fn new(local_public_key: &PublicKey,
               remote_public_key: &PublicKey,
               token_channel_index: u16) -> TokenChannelSlot {
        TokenChannelSlot {
            directional: DirectionalTokenChannel::new(local_public_key,
                                           remote_public_key,
                                           token_channel_index),
            tc_status: TokenChannelStatus::Valid,
            // The remote_max_debt we want to have. When possible, this will be sent to the remote
            // side.
            wanted_remote_max_debt: 0,
            // The local_send_price we want to have (Or possibly close requests, by having an empty
            // send price). When possible, this will be updated with the TokenChannel.
            wanted_local_send_price: None,
            pending_operations: Vector::new(),
            pending_send_funds_id: None,
        }
    }

    pub fn new_from_reset(local_public_key: &PublicKey,
                           remote_public_key: &PublicKey,
                           token_channel_index: u16,
                           current_token: &ChannelToken,
                           balance: i64) -> TokenChannelSlot {

        let directional = DirectionalTokenChannel::new_from_reset(local_public_key,
                                                      remote_public_key,
                                                      token_channel_index,
                                                      current_token,
                                                      balance);

        // Set what we want to be what we have:
        let wanted_remote_max_debt 
            = directional.token_channel.state().balance.remote_max_debt;
        let wanted_local_send_price 
            = directional.token_channel.state().send_price.local_send_price.clone();

        TokenChannelSlot {
            directional,
            tc_status: TokenChannelStatus::Valid,
            wanted_remote_max_debt,
            wanted_local_send_price,
            pending_operations: Vector::new(),
            pending_send_funds_id: None,
        }
    }

    #[allow(unused)]
    pub fn mutate(&mut self, slot_mutation: &SlotMutation) {
        match slot_mutation {
            SlotMutation::DirectionalMutation(directional_mutation) => {
                self.directional.mutate(directional_mutation);
            },
            SlotMutation::SetTcStatus(tc_status) => {
                self.tc_status = tc_status.clone();
            },
            SlotMutation::SetWantedRemoteMaxDebt(wanted_remote_max_debt) => {
                self.wanted_remote_max_debt = *wanted_remote_max_debt;
            },
            SlotMutation::SetWantedLocalSendPrice(wanted_local_send_price) => {
                self.wanted_local_send_price = wanted_local_send_price.clone();
            },
            SlotMutation::PushBackPendingOperation(neighbor_tc_op) => {
                self.pending_operations.push_back(neighbor_tc_op.clone());
            },
            SlotMutation::PopFrontPendingOperation => {
                let _ = self.pending_operations.pop_front();
            },
            SlotMutation::SetPendingSendFundsId(request_id) => {
                self.pending_send_funds_id = Some(request_id.clone());
            },
            SlotMutation::ClearPendingSendFundsId => {
                self.pending_send_funds_id = None;
            },
            SlotMutation::LocalReset => {
                match &self.tc_status {
                    TokenChannelStatus::Valid => unreachable!(),
                    TokenChannelStatus::Inconsistent {current_token, balance_for_reset} => {
                        self.directional = DirectionalTokenChannel::new_from_reset(
                            &self.directional.token_channel.state().idents.local_public_key,
                            &self.directional.token_channel.state().idents.remote_public_key,
                            self.directional.token_channel_index,
                            &current_token,
                            *balance_for_reset);
                    }
                }
            },
            SlotMutation::RemoteReset => {
                let reset_token = self.directional.calc_channel_reset_token(
                    self.directional.token_channel_index);
                let balance_for_reset = self.directional.balance_for_reset();
                self.tc_status = TokenChannelStatus::Valid;
                self.directional = DirectionalTokenChannel::new_from_reset(
                    &self.directional.token_channel.state().idents.local_public_key,
                    &self.directional.token_channel.state().idents.remote_public_key,
                    self.directional.token_channel_index,
                    &reset_token,
                    balance_for_reset);
            },
        }
    }
}
