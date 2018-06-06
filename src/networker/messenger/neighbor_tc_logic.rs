#![warn(unused)]

use proto::networker::ChannelToken;
use crypto::rand_values::RandValue;
use networker::messages::MoveTokenDirection;
use super::token_channel::{TokenChannel, ProcessMessageOutput, 
    ProcessTransListError, atomic_process_operations_list};
use super::types::NeighborTcOp;

pub struct NeighborMoveToken {
    pub operations: Vec<NeighborTcOp>,
    pub old_token: ChannelToken,
    pub rand_nonce: RandValue,
}


struct ChainState {
    pub direction: MoveTokenDirection,
    pub old_token: ChannelToken,
    pub new_token: ChannelToken,
    // Equals Sha512/256(NeighborMoveToken)
}


pub struct NeighborTCState {
    chain_state: ChainState,
    token_channel: TokenChannel,
}

#[derive(Debug)]
pub enum NeighborTCStateError {
    ChainInconsistency,
    InvalidTransaction(ProcessTransListError),
}

pub enum ReceiveTokenOutput {
    Duplicate,
    RetransmitOutgoing,
    ProcessTransListOutput(Vec<ProcessMessageOutput>),
}


pub fn receive_move_token(neighbor_tc_state: NeighborTCState, 
                          move_token_message: NeighborMoveToken, 
                          new_token: ChannelToken) 
    -> (NeighborTCState, Result<ReceiveTokenOutput, NeighborTCStateError>) {


    match neighbor_tc_state.chain_state.direction {
        MoveTokenDirection::Incoming => {
            if new_token == neighbor_tc_state.chain_state.new_token {
                // Duplicate
                (neighbor_tc_state, Ok(ReceiveTokenOutput::Duplicate))
            } else {
                // Inconsistency
                (neighbor_tc_state, Err(NeighborTCStateError::ChainInconsistency))
            }
        },
        MoveTokenDirection::Outgoing => {
            if move_token_message.old_token == neighbor_tc_state.chain_state.new_token {
                match atomic_process_operations_list(neighbor_tc_state.token_channel, 
                                                move_token_message.operations) {
                    (token_channel, Ok(output)) => {
                        // If processing the transactions was successful, we 
                        // set old_token, new_token and direction:
                        let neighbor_tc_state = NeighborTCState {
                            chain_state: ChainState {
                                old_token: neighbor_tc_state.chain_state.new_token.clone(),
                                new_token,
                                direction: MoveTokenDirection::Incoming,
                            },
                            token_channel,
                        };
                        (neighbor_tc_state, Ok(ReceiveTokenOutput::ProcessTransListOutput(output)))
                    },
                    (token_channel, Err(e)) => {
                        let neighbor_tc_state = NeighborTCState {
                            chain_state: neighbor_tc_state.chain_state,
                            token_channel,
                        };
                        (neighbor_tc_state, Err(NeighborTCStateError::InvalidTransaction(e)))
                    },
                }
            } else if neighbor_tc_state.chain_state.old_token == new_token {
                // We should retransmit send our message to the remote side.
                (neighbor_tc_state, Ok(ReceiveTokenOutput::RetransmitOutgoing))
            } else {
                (neighbor_tc_state, Err(NeighborTCStateError::ChainInconsistency))
            }
        },
    }
}

#[allow(unused)]
pub fn send_move_token(neighbor_tc_state: NeighborTCState, move_token_message: &NeighborMoveToken, new_token: ChannelToken) -> 
    (NeighborTCState, Result<(), ()>) {
    
    // TODO:

    (neighbor_tc_state, Ok(()))
}

