use im::vector::Vector;

use crypto::identity::PublicKey;
use crypto::uid::Uid;

use super::token_channel::directional::DirectionalMutation;
use proto::funder::ChannelToken;
use super::types::FriendTcOp;
use super::token_channel::directional::DirectionalTokenChannel;
use super::messages::FriendStatus;


#[derive(Clone)]
pub struct ResetTerms {
    pub current_token: ChannelToken,
    pub balance_for_reset: i128,
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
pub enum FriendMutation {
    DirectionalMutation(DirectionalMutation),
    SetIncomingInconsistency(IncomingInconsistency),
    SetOutgoingInconsistency(OutgoingInconsistency),
    SetWantedRemoteMaxDebt(u128),
    PushBackPendingOperation(FriendTcOp),
    PopFrontPendingOperation,
    SetStatus(FriendStatus),
    RemoteReset,        // Remote side performed reset
    LocalReset,         // Local side performed reset
}

#[allow(unused)]
#[derive(Clone)]
pub struct FriendState<A> {
    pub opt_remote_address: Option<A>, 
    pub directional: DirectionalTokenChannel,
    pub inconsistency_status: InconsistencyStatus,
    pub wanted_remote_max_debt: u128,
    pub pending_operations: Vector<FriendTcOp>,
    // Pending operations to be sent to the token channel.
    pub status: FriendStatus,
}


#[allow(unused)]
impl<A> FriendState<A> {
    pub fn new(local_public_key: &PublicKey,
               remote_public_key: &PublicKey,
               opt_remote_address: Option<A>) -> FriendState<A> {
        FriendState {
            opt_remote_address,
            directional: DirectionalTokenChannel::new(local_public_key,
                                           remote_public_key),

            inconsistency_status: InconsistencyStatus::new(),
            // The remote_max_debt we want to have. When possible, this will be sent to the remote
            // side.
            wanted_remote_max_debt: 0,
            // The local_send_price we want to have (Or possibly close requests, by having an empty
            // send price). When possible, this will be updated with the TokenChannel.
            pending_operations: Vector::new(),
            status: FriendStatus::Enable,
        }
    }


    #[allow(unused)]
    pub fn mutate(&mut self, slot_mutation: &FriendMutation) {
        match slot_mutation {
            FriendMutation::DirectionalMutation(directional_mutation) => {
                self.directional.mutate(directional_mutation);
            },
            FriendMutation::SetIncomingInconsistency(incoming_inconsistency) => {
                self.inconsistency_status.incoming = incoming_inconsistency.clone();
            },
            FriendMutation::SetOutgoingInconsistency(outgoing_inconsistency) => {
                self.inconsistency_status.outgoing = outgoing_inconsistency.clone();
            },
            FriendMutation::SetWantedRemoteMaxDebt(wanted_remote_max_debt) => {
                self.wanted_remote_max_debt = *wanted_remote_max_debt;
            },
            FriendMutation::PushBackPendingOperation(friend_tc_op) => {
                self.pending_operations.push_back(friend_tc_op.clone());
            },
            FriendMutation::PopFrontPendingOperation => {
                let _ = self.pending_operations.pop_front();
            },
            FriendMutation::SetStatus(friend_status) => {
                self.status = friend_status.clone();
            },
            FriendMutation::LocalReset => {
                // Local reset was applied (We sent a reset from AppManager).
                match &self.inconsistency_status.incoming {
                    IncomingInconsistency::Empty => unreachable!(),
                    IncomingInconsistency::Incoming(reset_terms) => {
                        self.directional = DirectionalTokenChannel::new_from_reset(
                            &self.directional.token_channel.state().idents.local_public_key,
                            &self.directional.token_channel.state().idents.remote_public_key,
                            &reset_terms.current_token,
                            reset_terms.balance_for_reset);
                    }
                };
                self.inconsistency_status = InconsistencyStatus::new();
            },
            FriendMutation::RemoteReset => {
                // Remote reset was applied (Remote side has given a reset command)
                let reset_token = self.directional.calc_channel_reset_token();
                let balance_for_reset = self.directional.balance_for_reset();
                self.inconsistency_status = InconsistencyStatus::new();
                self.directional = DirectionalTokenChannel::new_from_reset(
                    &self.directional.token_channel.state().idents.local_public_key,
                    &self.directional.token_channel.state().idents.remote_public_key,
                    &reset_token,
                    balance_for_reset);
            },
        }
    }
}
