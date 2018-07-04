#![warn(unused)]

use std::convert::TryFrom;
use byteorder::{BigEndian, WriteBytesExt};

use ring::rand::SecureRandom;

use proto::networker::ChannelToken;
use crypto::identity::PublicKey;
use crypto::rand_values::{RandValue, RAND_VALUE_LEN};
use crypto::hash::sha_512_256;

use super::types::{TokenChannel, NeighborMoveTokenInner};
use super::incoming::{atomic_process_operations_list, 
    ProcessOperationOutput, ProcessTransListError};
use super::outgoing::{OutgoingTokenChannel, QueueOperationFailure};

use super::super::types::NeighborTcOp;

// Prefix used for chain hashing of token channel messages.
// NEXT is used for hashing for the next move token message.
// RESET is used for resetting the token channel.
// The prefix allows the receiver to distinguish between the two cases.
const TOKEN_NEXT: &[u8] = b"NEXT";
const TOKEN_RESET: &[u8] = b"RESET";


/// Indicate the direction of the move token message.
pub enum MoveTokenDirection {
    Incoming,
    Outgoing(NeighborMoveTokenInner),
}


pub struct DirectionalTokenChannel {
    direction: MoveTokenDirection,
    new_token: ChannelToken,
    // Equals Sha512/256(NeighborMoveToken)
    opt_token_channel: Option<TokenChannel>,
}

#[derive(Debug)]
pub enum ReceiveMoveTokenError {
    ChainInconsistency,
    InvalidTransaction(ProcessTransListError),
}

pub enum ReceiveMoveTokenOutput {
    Duplicate,
    RetransmitOutgoing,
    ProcessOpsListOutput(Vec<ProcessOperationOutput>),
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

/// Calculate the token to be used for resetting the channel.
#[allow(unused)]
pub fn calc_channel_reset_token(token_channel_index: u16,
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

#[allow(unused)]
struct TokenChannelSender<'a> {
    directional_tc: &'a mut DirectionalTokenChannel,
    outgoing_tc: OutgoingTokenChannel,
}


#[allow(unused)]
impl<'a> TokenChannelSender<'a> {
    pub fn new(directional_tc: &'a mut DirectionalTokenChannel) -> Self {
        let outgoing_tc = OutgoingTokenChannel::new(
            directional_tc.opt_token_channel.take().expect("token channel not present!"));
        TokenChannelSender {
            directional_tc, 
            outgoing_tc,
        }
    }

    pub fn queue_operation(&mut self, operation: NeighborTcOp) ->
        Result<(), QueueOperationFailure> {
        
        self.outgoing_tc.queue_operation(operation)
    }

    pub fn commit<R: SecureRandom>(self, rng: &R) {
        let Self {mut directional_tc, outgoing_tc} = self;

        let (token_channel, operations) = outgoing_tc.commit();
        directional_tc.opt_token_channel = Some(token_channel);

        let neighbor_move_token_inner = NeighborMoveTokenInner {
            operations,
            old_token: directional_tc.new_token.clone(),
            rand_nonce: RandValue::new(rng),
        };

        directional_tc.direction = MoveTokenDirection::Outgoing(
            neighbor_move_token_inner);


        // TODO: Calculate new value for directional_tc.new_token:
        // Required: A way to serialize all operations, to obtain the value for contents argument.
        // directional_tc.new_token = 
        unreachable!();
        
    }
}

/*
impl<'a> Drop for TokenChannelSender<'a> {
    fn drop(&mut self) {
        self.directional_tc.opt_token_channel 
            = Some(self.opt_outgoing_tc.take().commit());
    }
}
*/


impl DirectionalTokenChannel {
    #[allow(unused)]
    pub fn new(local_public_key: &PublicKey, 
               remote_public_key: &PublicKey,
               token_channel_index: u16) -> DirectionalTokenChannel {

        let mut hash_buffer: Vec<u8> = Vec::new();

        let local_pk_hash = sha_512_256(local_public_key);
        let remote_pk_hash = sha_512_256(remote_public_key);
        let new_token_channel = TokenChannel::new(local_public_key, remote_public_key, 0);

        let rand_nonce = RandValue::try_from(&remote_pk_hash.as_ref()[.. RAND_VALUE_LEN])
                    .expect("Failed to trim a public key hash into the size of random value!");

        // Calculate hash(FirstMoveTokenLower):
        let new_token = calc_channel_next_token(token_channel_index,
                                           &[],
                                           &ChannelToken::from(local_pk_hash.as_array_ref()),
                                           &rand_nonce);

        if local_pk_hash < remote_pk_hash {
            // We are the first sender
            DirectionalTokenChannel {
                direction: MoveTokenDirection::Outgoing(NeighborMoveTokenInner {
                    operations: Vec::new(),
                    old_token: ChannelToken::from(local_pk_hash.as_array_ref()),
                    rand_nonce,
                }),
                new_token,
                opt_token_channel: Some(new_token_channel),
            }
        } else {
            // We are the second sender
            DirectionalTokenChannel {
                direction: MoveTokenDirection::Incoming,
                new_token,
                opt_token_channel: Some(new_token_channel),
            }
        }
    }

    pub fn new_from_reset(local_public_key: &PublicKey, 
                      remote_public_key: &PublicKey, 
                      current_token: &ChannelToken, 
                      balance: i64) -> DirectionalTokenChannel {
        DirectionalTokenChannel {
            direction: MoveTokenDirection::Incoming,
            new_token: current_token.clone(),
            opt_token_channel: Some(TokenChannel::new(local_public_key, remote_public_key, balance)),
        }
    }

    /// Get a reference to internal token_channel.
    pub fn get_token_channel(&self) -> &TokenChannel {
        if let Some(ref token_channel) = self.opt_token_channel {
            token_channel
        } else {
            panic!("token_channel is not present!");
        }
    }

    #[allow(unused)]
    pub fn balance_for_reset(&self) -> i64 {
        self.get_token_channel().balance_for_reset()
    }

    #[allow(unused)]
    pub fn calc_channel_reset_token(&self, token_channel_index: u16) -> ChannelToken {
        calc_channel_reset_token(token_channel_index,
                                     &self.new_token,
                                     self.get_token_channel().balance_for_reset())
    }


    #[allow(unused)]
    pub fn receive_move_token(&mut self, 
                              move_token_message: NeighborMoveTokenInner, 
                              new_token: ChannelToken) 
        -> Result<ReceiveMoveTokenOutput, ReceiveMoveTokenError> {

        match self.direction {
            MoveTokenDirection::Incoming => {
                if new_token == self.new_token {
                    // Duplicate
                    Ok(ReceiveMoveTokenOutput::Duplicate)
                } else {
                    // Inconsistency
                    Err(ReceiveMoveTokenError::ChainInconsistency)
                }
            },
            MoveTokenDirection::Outgoing(ref move_token_inner) => {
                if move_token_message.old_token == self.new_token {
                    let token_channel = self.opt_token_channel
                        .take()
                        .expect("TokenChannel not present!");
                    match atomic_process_operations_list(token_channel, 
                                                    move_token_message.operations) {
                        (token_channel, Ok(output)) => {
                            // If processing the transactions was successful, we 
                            // set old_token, new_token and direction:
                            self.opt_token_channel = Some(token_channel);
                            self.direction = MoveTokenDirection::Incoming;
                            self.new_token = new_token;
                            Ok(ReceiveMoveTokenOutput::ProcessOpsListOutput(output))
                        },
                        (token_channel, Err(e)) => {
                            self.opt_token_channel = Some(token_channel);
                            Err(ReceiveMoveTokenError::InvalidTransaction(e))
                        },
                    }
                } else if move_token_inner.old_token == new_token {
                    // We should retransmit our message to the remote side.
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
