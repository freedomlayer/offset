#![warn(unused)]

use proto::networker::ChannelToken;
use crypto::identity::PublicKey;
use crypto::rand_values::RandValue;
use networker::messages::MoveTokenDirection;
use super::token_channel::{TokenChannel, ProcessOperationOutput, 
    ProcessTransListError, atomic_process_operations_list};
use super::types::NeighborTcOp;

pub struct NeighborMoveTokenInner {
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
    token_channel: Option<TokenChannel>,
}

#[derive(Debug)]
pub enum ReceiveMoveTokenError {
    ChainInconsistency,
    InvalidTransaction(ProcessTransListError),
}

pub enum ReceiveMoveTokenOutput {
    Duplicate,
    RetransmitOutgoing,
    ProcessTransListOutput(Vec<ProcessOperationOutput>),
}



impl NeighborTCState {
    pub fn new_from_reset(local_public_key: &PublicKey, 
                      remote_public_key: &PublicKey, 
                      current_token: &ChannelToken, 
                      balance: i64) -> NeighborTCState {
        NeighborTCState {
            chain_state: ChainState {
                direction: MoveTokenDirection::Incoming,
                old_token: current_token.clone(), // TODO: What to put here?
                new_token: current_token.clone(),
            },
            token_channel: Some(TokenChannel::new(local_public_key, remote_public_key, balance)),
        }
    }

    #[allow(unused)]
    pub fn receive_move_token(&mut self, 
                              move_token_message: NeighborMoveTokenInner, 
                              new_token: ChannelToken) 
        -> Result<ReceiveMoveTokenOutput, ReceiveMoveTokenError> {

        match self.chain_state.direction {
            MoveTokenDirection::Incoming => {
                if new_token == self.chain_state.new_token {
                    // Duplicate
                    Ok(ReceiveMoveTokenOutput::Duplicate)
                } else {
                    // Inconsistency
                    Err(ReceiveMoveTokenError::ChainInconsistency)
                }
            },
            MoveTokenDirection::Outgoing => {
                if move_token_message.old_token == self.chain_state.new_token {
                    let token_channel = self.token_channel.take().expect("TokenChannel not present!");
                    match atomic_process_operations_list(token_channel, 
                                                    move_token_message.operations) {
                        (token_channel, Ok(output)) => {
                            // If processing the transactions was successful, we 
                            // set old_token, new_token and direction:
                            self.token_channel = Some(token_channel);
                            self.chain_state = ChainState {
                                old_token: self.chain_state.new_token.clone(),
                                new_token,
                                direction: MoveTokenDirection::Incoming,
                            };
                            Ok(ReceiveMoveTokenOutput::ProcessTransListOutput(output))
                        },
                        (token_channel, Err(e)) => {
                            self.token_channel = Some(token_channel);
                            Err(ReceiveMoveTokenError::InvalidTransaction(e))
                        },
                    }
                } else if self.chain_state.old_token == new_token {
                    // We should retransmit send our message to the remote side.
                    Ok(ReceiveMoveTokenOutput::RetransmitOutgoing)
                } else {
                    Err(ReceiveMoveTokenError::ChainInconsistency)
                }
            },
        }
    }

    #[allow(unused)]
    pub fn send_move_token(&mut self, 
                           move_token_message: &NeighborMoveTokenInner, 
                           new_token: ChannelToken) -> Result<(), ()> {
        
        // TODO:
        Ok(())
    }
}
