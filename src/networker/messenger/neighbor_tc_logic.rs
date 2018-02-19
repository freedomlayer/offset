use proto::networker::ChannelToken;
use crypto::rand_values::RandValue;
use super::super::messages::MoveTokenDirection;
use super::balance_state::{BalanceState, NetworkerTCTransaction, 
    ProcessTransListOutput, ProcessTransListError};

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


struct NeighborTCState {
    pub chain_state: ChainState,
    pub balance_state: BalanceState,
}

#[derive(Debug)]
pub enum NeighborTCStateError {
    ChainInconsistency,
    InvalidTransaction(ProcessTransListError),
}

pub enum ReceiveTokenOutput {
    Duplicate,
    RetransmitOutgoing,
    ProcessTransListOutput(ProcessTransListOutput),
}


impl NeighborTCState {
    pub fn receive_move_token(&mut self, move_token_message: &NeighborMoveToken, 
                              new_token: ChannelToken) 
        -> Result<ReceiveTokenOutput, NeighborTCStateError> {

        match self.chain_state.direction {
            MoveTokenDirection::Incoming => {
                if new_token == self.chain_state.new_token {
                    // Duplicate
                    Ok(ReceiveTokenOutput::Duplicate)
                } else {
                    // Inconsistency
                    Err(NeighborTCStateError::ChainInconsistency)
                }
            },
            MoveTokenDirection::Outgoing => {
                if move_token_message.old_token == self.chain_state.new_token {
                    match self.balance_state.process_trans_list_atomic(
                        &move_token_message.transactions) {
                        Ok(output) => {
                            // If processing the transactions was successful, we 
                            // set old_token, new_token and direction:
                            self.chain_state.old_token = self.chain_state.new_token.clone();
                            self.chain_state.new_token = new_token;
                            self.chain_state.direction = MoveTokenDirection::Incoming;
                            Ok(ReceiveTokenOutput::ProcessTransListOutput(output))
                        },
                        Err(e) => Err(NeighborTCStateError::InvalidTransaction(e)),
                    }
                } else if self.chain_state.old_token == new_token {
                    // We should retransmit send our message to the remote side.
                    Ok(ReceiveTokenOutput::RetransmitOutgoing)
                } else {
                    Err(NeighborTCStateError::ChainInconsistency)
                }
            },
        }
    }

    pub fn send_move_token(&mut self, move_token_message: &NeighborMoveToken, new_token: ChannelToken) -> Result<(), ()> {
        Ok(())
    }
}

