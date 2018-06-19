#![warn(unused)]

use std::convert::TryFrom;
use byteorder::{BigEndian, WriteBytesExt};

use proto::networker::ChannelToken;
use crypto::identity::PublicKey;
use crypto::rand_values::{RandValue, RAND_VALUE_LEN};
use crypto::hash::sha_512_256;
use networker::messages::MoveTokenDirection;
use super::token_channel::{TokenChannel, ProcessOperationOutput, 
    ProcessTransListError, atomic_process_operations_list};
use super::types::NeighborTcOp;

// Prefix used for chain hashing of token channel messages.
// NEXT is used for hashing for the next move token message.
// RESET is used for resetting the token channel.
// The prefix allows the receiver to distinguish between the two cases.
const TOKEN_NEXT: &[u8] = b"NEXT";
const TOKEN_RESET: &[u8] = b"RESET";

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

/// Calculate the next token channel, given values of previous NeighborMoveToken message.
fn calc_channel_next_token(token_channel_index: u16, 
                      contents: &[u8], 
                      old_token: &ChannelToken,
                      rand_nonce: &RandValue) -> ChannelToken {

    let mut hash_buffer = Vec::new();
    hash_buffer.extend_from_slice(&sha_512_256(TOKEN_NEXT));
    hash_buffer.write_u16::<BigEndian>(token_channel_index).expect("Error serializing u16");
    hash_buffer.extend_from_slice(contents);
    hash_buffer.extend_from_slice(old_token);
    hash_buffer.extend_from_slice(rand_nonce);
    let hash_result = sha_512_256(&hash_buffer);
    ChannelToken::from(hash_result.as_array_ref())
}

#[allow(unused)]
fn calc_channel_reset_token(token_channel_index: u16,
                      new_token: &ChannelToken,
                      balance_for_reset: i64) -> ChannelToken {

    let mut hash_buffer = Vec::new();
    hash_buffer.extend_from_slice(&sha_512_256(TOKEN_RESET));
    hash_buffer.write_u16::<BigEndian>(token_channel_index).expect("Error serializing u16");
    hash_buffer.extend_from_slice(&new_token);
    hash_buffer.write_i64::<BigEndian>(balance_for_reset).expect("Error serializing i64");
    let hash_result = sha_512_256(&hash_buffer);
    ChannelToken::from(hash_result.as_array_ref())
}


impl NeighborTCState {
    #[allow(unused)]
    pub fn new(local_public_key: &PublicKey, 
               remote_public_key: &PublicKey,
               token_channel_index: u16) -> NeighborTCState {

        let mut hash_buffer: Vec<u8> = Vec::new();

        let local_pk_hash = sha_512_256(local_public_key);
        let remote_pk_hash = sha_512_256(remote_public_key);
        let new_token_channel = TokenChannel::new(local_public_key, remote_public_key, 0);

        if local_pk_hash < remote_pk_hash {
            // We are the first sender
            NeighborTCState {
                chain_state: ChainState {
                    direction: MoveTokenDirection::Outgoing,
                    old_token: ChannelToken::from(local_pk_hash.as_array_ref()),
                    new_token: ChannelToken::from(remote_pk_hash.as_array_ref()),
                },
                token_channel: Some(new_token_channel),
            }
        } else {
            // We are the second sender
            // Calculate hash(FirstMoveTokenLower):
            let rand_value = RandValue::try_from(&remote_pk_hash.as_ref()[.. RAND_VALUE_LEN])
                .expect("Failed to trim a public key hash into the size of random value!");
            let new_token = calc_channel_next_token(token_channel_index,
                                               &[],
                                               &ChannelToken::from(local_pk_hash.as_array_ref()),
                                               &rand_value);

            NeighborTCState {
                chain_state: ChainState {
                    direction: MoveTokenDirection::Incoming,
                    old_token: new_token.clone(), // TODO: What to put here?
                    new_token,
                },
                token_channel: Some(new_token_channel),
            }
        }
    }

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
