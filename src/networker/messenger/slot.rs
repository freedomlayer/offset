use im::vector::Vector;

use crypto::identity::PublicKey;
use crypto::uid::Uid;

use super::token_channel::directional::DirectionalMutation;
use proto::networker::ChannelToken;
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
    PushBackPendingOperation(NeighborTcOp),
    PopFrontPendingOperation,
    SetPendingSendFundsId(Uid),
    ClearPendingSendFundsId,
}

#[allow(unused)]
#[derive(Clone)]
pub struct TokenChannelSlot {
    pub directional: DirectionalTokenChannel,
    pub tc_status: TokenChannelStatus,
    pub wanted_remote_max_debt: u64,
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
            wanted_remote_max_debt: 0,
            pending_operations: Vector::new(),
            pending_send_funds_id: None,
        }
    }

    pub fn new_from_reset(local_public_key: &PublicKey,
                           remote_public_key: &PublicKey,
                           token_channel_index: u16,
                           current_token: &ChannelToken,
                           balance: i64) -> TokenChannelSlot {

        TokenChannelSlot {
            directional: DirectionalTokenChannel::new_from_reset(local_public_key,
                                                      remote_public_key,
                                                      token_channel_index,
                                                      current_token,
                                                      balance),
            tc_status: TokenChannelStatus::Valid,
            wanted_remote_max_debt: 0,
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
            SlotMutation::PushBackPendingOperation(neighbor_tc_op) => {
                self.pending_operations.push_back(neighbor_tc_op.clone());
            },
            SlotMutation::PopFrontPendingOperation => {
                let _ = self.pending_operations.pop_front();
            },
            SlotMutation::SetPendingSendFundsId(request_id) => {
                self.pending_send_funds = Some(requet_id.clone());
            },
            SlotMutation::ClearPendingSendFundsId => {
                self.pending_send_funds = None;
            },
        }
    }
}
