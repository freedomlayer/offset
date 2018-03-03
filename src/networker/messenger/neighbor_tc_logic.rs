use proto::networker::ChannelToken;
use crypto::rand_values::RandValue;
use crypto::identity::PublicKey;
use super::super::messages::MoveTokenDirection;
use super::balance_state::{BalanceState, NetworkerTCTransaction, 
    ProcessTransOutput, ProcessTransListError,
    atomic_process_trans_list};

pub struct NeighborMoveToken {
    // pub channel_index: u32,
    pub transactions: Vec<NetworkerTCTransaction>,
    pub old_token: ChannelToken,
    pub rand_nonce: RandValue,
}

struct ChainState {
    pub direction: MoveTokenDirection,
    pub old_token: ChannelToken,
    pub new_token: ChannelToken,
    // Equals Sha512/256(move_token_message)
}


pub struct NeighborTCState {
    chain_state: ChainState,
    balance_state: BalanceState,
}

#[derive(Debug)]
pub enum NeighborTCStateError {
    ChainInconsistency,
    InvalidTransaction(ProcessTransListError),
}

pub enum ReceiveTokenOutput {
    Duplicate,
    RetransmitOutgoing,
    ProcessTransListOutput(Vec<ProcessTransOutput>),
}


pub fn receive_move_token(neighbor_tc_state: NeighborTCState, 
                          local_public_key: &PublicKey,
                          remote_public_key: &PublicKey,
                          move_token_message: &NeighborMoveToken, 
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
                match atomic_process_trans_list(neighbor_tc_state.balance_state, 
                                                local_public_key,
                                                remote_public_key,
                                                &move_token_message.transactions) {
                    (balance_state, Ok(output)) => {
                        // If processing the transactions was successful, we 
                        // set old_token, new_token and direction:
                        let neighbor_tc_state = NeighborTCState {
                            chain_state: ChainState {
                                old_token: neighbor_tc_state.chain_state.new_token.clone(),
                                new_token,
                                direction: MoveTokenDirection::Incoming,
                            },
                            balance_state,
                        };
                        (neighbor_tc_state, Ok(ReceiveTokenOutput::ProcessTransListOutput(output)))
                    },
                    (balance_state, Err(e)) => {
                        let neighbor_tc_state = NeighborTCState {
                            chain_state: neighbor_tc_state.chain_state,
                            balance_state,
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

#[allow(unused)] //TODO(a4vision): implement.
pub fn send_move_token(neighbor_tc_state: NeighborTCState, move_token_message: &NeighborMoveToken, new_token: ChannelToken) -> 
    (NeighborTCState, Result<(), ()>) {

    (neighbor_tc_state, Ok(()))
}

