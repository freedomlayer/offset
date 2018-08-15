use im::vector::Vector;

use crypto::identity::PublicKey;
use crypto::uid::Uid;

use super::token_channel::directional::DirectionalMutation;
use proto::networker::{ChannelToken, NetworkerSendPrice};
use super::types::{FriendTcOp};
use super::token_channel::directional::{DirectionalTokenChannel};

/*

#[derive(Clone)]
pub struct StatusInconsistent {
    // Has the remote side acknowledged our reset terms?
    pub local_terms_acked: bool,
    pub current_token: ChannelToken,
    pub balance_for_reset: i64,
}

#[allow(dead_code)]
#[derive(Clone)]
pub enum TokenChannelStatus {
    Valid,
    /// Inconsistent means that the remote side showed disagreement about the 
    /// token channel, and this channel is waiting for a local human intervention.
    /// The information in Inconsistent are the requirements of the remote side for a reset.
    Inconsistent(StatusInconsistent),
}
*/

#[derive(Clone)]
pub struct ResetTerms {
    pub current_token: ChannelToken,
    pub balance_for_reset: i64,
}

#[derive(Clone)]
pub enum IncomingInconsistency {
    // No incoming inconsistency was received
    Empty,
    // Incoming inconsistency was received from remote side.
    Incoming(ResetTerms),
}

#[derive(Clone)]
pub enum OutgoingInconsistency {
    // No outgoing inconsistency in progress
    Empty,
    // Outgoing inconsistency message was sent
    Sent,
    // Outgoing inconsistency message was sent and acknowledged by remote side
    Acked,
}

#[derive(Clone)]
pub struct InconsistencyStatus {
    pub incoming: IncomingInconsistency,
    pub outgoing: OutgoingInconsistency,
}

impl InconsistencyStatus {
    pub fn new() -> InconsistencyStatus {
        InconsistencyStatus {
            incoming: IncomingInconsistency::Empty,
            outgoing: OutgoingInconsistency::Empty,
        }
    }
}

#[allow(unused)]
pub enum SlotMutation {
    DirectionalMutation(DirectionalMutation),
    SetIncomingInconsistency(IncomingInconsistency),
    SetOutgoingInconsistency(OutgoingInconsistency),
    SetWantedRemoteMaxDebt(u64),
    SetWantedLocalSendPrice(Option<NetworkerSendPrice>),
    PushBackPendingOperation(FriendTcOp),
    PopFrontPendingOperation,
    SetPendingSendFundsId(Option<Uid>),
    RemoteReset,        // Remote side performed reset
    LocalReset,         // Local side performed reset
}

#[allow(unused)]
#[derive(Clone)]
pub struct TokenChannelSlot {
    pub directional: DirectionalTokenChannel,
    pub inconsistency_status: InconsistencyStatus,
    pub wanted_remote_max_debt: u64,
    pub wanted_local_send_price: Option<NetworkerSendPrice>,
    pub pending_operations: Vector<FriendTcOp>,
    // Pending operations to be sent to the token channel.
    pub opt_pending_send_funds_id: Option<Uid>,
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

            inconsistency_status: InconsistencyStatus::new(),
            // The remote_max_debt we want to have. When possible, this will be sent to the remote
            // side.
            wanted_remote_max_debt: 0,
            // The local_send_price we want to have (Or possibly close requests, by having an empty
            // send price). When possible, this will be updated with the TokenChannel.
            wanted_local_send_price: None,
            pending_operations: Vector::new(),
            opt_pending_send_funds_id: None,
        }
    }

    /*
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
            opt_pending_send_funds_id: None, // TODO: What to put here?
        }
    }
    */

    #[allow(unused)]
    pub fn mutate(&mut self, slot_mutation: &SlotMutation) {
        match slot_mutation {
            SlotMutation::DirectionalMutation(directional_mutation) => {
                self.directional.mutate(directional_mutation);
            },
            SlotMutation::SetIncomingInconsistency(incoming_inconsistency) => {
                self.inconsistency_status.incoming = incoming_inconsistency.clone();
            },
            SlotMutation::SetOutgoingInconsistency(outgoing_inconsistency) => {
                self.inconsistency_status.outgoing = outgoing_inconsistency.clone();
            },
            SlotMutation::SetWantedRemoteMaxDebt(wanted_remote_max_debt) => {
                self.wanted_remote_max_debt = *wanted_remote_max_debt;
            },
            SlotMutation::SetWantedLocalSendPrice(wanted_local_send_price) => {
                self.wanted_local_send_price = wanted_local_send_price.clone();
            },
            SlotMutation::PushBackPendingOperation(friend_tc_op) => {
                self.pending_operations.push_back(friend_tc_op.clone());
            },
            SlotMutation::PopFrontPendingOperation => {
                let _ = self.pending_operations.pop_front();
            },
            SlotMutation::SetPendingSendFundsId(opt_request_id) => {
                // We don't want to lose another payment:
                assert!(opt_request_id.is_none());
                self.opt_pending_send_funds_id = *opt_request_id;
            },
            SlotMutation::LocalReset => {
                // Local reset was applied (We sent a reset from AppManager).
                match &self.inconsistency_status.incoming {
                    IncomingInconsistency::Empty => unreachable!(),
                    IncomingInconsistency::Incoming(reset_terms) => {
                        self.directional = DirectionalTokenChannel::new_from_reset(
                            &self.directional.token_channel.state().idents.local_public_key,
                            &self.directional.token_channel.state().idents.remote_public_key,
                            self.directional.token_channel_index,
                            &reset_terms.current_token,
                            reset_terms.balance_for_reset);
                    }
                };
                self.inconsistency_status = InconsistencyStatus::new();
            },
            SlotMutation::RemoteReset => {
                // Remote reset was applied (Remote side has given a reset command)
                let reset_token = self.directional.calc_channel_reset_token(
                    self.directional.token_channel_index);
                let balance_for_reset = self.directional.balance_for_reset();
                self.inconsistency_status = InconsistencyStatus::new();
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
