#![warn(unused)]

use std::convert::TryFrom;
use byteorder::{BigEndian, WriteBytesExt};

use proto::networker::ChannelToken;
use crypto::identity::PublicKey;
use crypto::rand_values::{RandValue, RAND_VALUE_LEN};
use crypto::hash::sha_512_256;

use utils::int_convert::usize_to_u64;

use super::types::{TokenChannel, NeighborMoveTokenInner};
use super::incoming::{atomic_process_operations_list, 
    ProcessOperationOutput, ProcessTransListError};
use super::outgoing::{OutgoingTokenChannel, QueueOperationFailure};

use super::super::types::{NeighborTcOp, NeighborMoveToken};


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
    token_channel_index: u16,
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
    RetransmitOutgoing(NeighborMoveToken),
    ProcessOpsListOutput(Vec<ProcessOperationOutput>),
}

pub struct TokenChannelSender {
    outgoing_tc: OutgoingTokenChannel,
}

/// Calculate the next token channel, given values of previous NeighborMoveToken message.
fn calc_channel_next_token(token_channel_index: u16, 
                      move_token_message: &NeighborMoveTokenInner) 
                        -> ChannelToken {

    let mut contents = Vec::new();
    contents.write_u64::<BigEndian>(
        usize_to_u64(move_token_message.operations.len()).unwrap()).unwrap();
    for op in &move_token_message.operations {
        contents.extend_from_slice(&op.to_bytes());
    }

    let mut hash_buffer = Vec::new();
    hash_buffer.extend_from_slice(&sha_512_256(TOKEN_NEXT));
    hash_buffer.write_u16::<BigEndian>(token_channel_index).expect("Error serializing u16");
    hash_buffer.extend_from_slice(&contents);
    hash_buffer.extend_from_slice(&move_token_message.old_token);
    hash_buffer.extend_from_slice(&move_token_message.rand_nonce);
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

impl TokenChannelSender {
    pub fn new(outgoing_tc: OutgoingTokenChannel) -> Self {
        TokenChannelSender {
            outgoing_tc,
        }
    }

    pub fn queue_operation(&mut self, operation: NeighborTcOp) ->
        Result<(), QueueOperationFailure> {
        self.outgoing_tc.queue_operation(operation)
    }

    pub fn is_empty(&self) -> bool {
        self.outgoing_tc.is_operations_empty()
    }
}



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

        let first_move_token_lower = NeighborMoveTokenInner {
            operations: Vec::new(),
            old_token: ChannelToken::from(local_pk_hash.as_array_ref()),
            rand_nonce: rand_nonce.clone(),
        };

        // Calculate hash(FirstMoveTokenLower):
        let new_token = calc_channel_next_token(token_channel_index,
                                                &first_move_token_lower);

        if local_pk_hash < remote_pk_hash {
            // We are the first sender
            DirectionalTokenChannel {
                token_channel_index,
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
                token_channel_index,
                direction: MoveTokenDirection::Incoming,
                new_token,
                opt_token_channel: Some(new_token_channel),
            }
        }
    }

    pub fn new_from_reset(local_public_key: &PublicKey, 
                      remote_public_key: &PublicKey, 
                      token_channel_index: u16,
                      current_token: &ChannelToken, 
                      balance: i64) -> DirectionalTokenChannel {
        DirectionalTokenChannel {
            token_channel_index,
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

    pub fn remote_max_debt(&self) -> u64 {
        self.get_token_channel().balance.remote_max_debt
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

        // Make sure that the given new_token is valid:
        let expected_new_token = calc_channel_next_token(self.token_channel_index,
                                                &move_token_message);
        if expected_new_token != new_token {
            return Err(ReceiveMoveTokenError::ChainInconsistency);
        }

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
                    
                    // TODO: Remove this expect somehow:
                    let outgoing_move_token = self.get_outgoing_move_token()
                        .expect("Can not obtain outgoing move token");
                    Ok(ReceiveMoveTokenOutput::RetransmitOutgoing(outgoing_move_token))
                } else {
                    Err(ReceiveMoveTokenError::ChainInconsistency)
                }
            },
        }
    }

    #[allow(unused)]
    pub fn begin_outgoing_move_token(&mut self) -> Option<TokenChannelSender> {
        if let MoveTokenDirection::Outgoing(_) = self.direction {
            return None;
        }

        let outgoing_tc = OutgoingTokenChannel::new(
            self.opt_token_channel.take()?);
    
        Some(TokenChannelSender::new(outgoing_tc))
    }

    #[allow(unused)]
    pub fn commit_outgoing_move_token(&mut self, 
                                  tc_sender: TokenChannelSender,
                                  rand_nonce: RandValue) -> NeighborMoveToken {
        if let MoveTokenDirection::Outgoing(_) = self.direction {
            panic!("Already in outgoing message state!");
        }
        let TokenChannelSender {outgoing_tc} = tc_sender;
        let (token_channel, operations) = outgoing_tc.commit();
        self.opt_token_channel = Some(token_channel);
        let neighbor_move_token_inner = NeighborMoveTokenInner {
            operations: operations,
            old_token: self.new_token.clone(),
            rand_nonce,
        };

        self.new_token = calc_channel_next_token(self.token_channel_index,
                                &neighbor_move_token_inner);
        self.direction = MoveTokenDirection::Outgoing(
            neighbor_move_token_inner);


        self.get_outgoing_move_token()
            .expect("Could not obtain outgoing move token!")
    }

    /*

    #[allow(unused)]
    fn send_move_token_transact<F>(&mut self, f: F, rand_nonce: RandValue) 
    where F: FnOnce(&mut TokenChannelSender) {

        match self.direction {
            MoveTokenDirection::Incoming => {},
            MoveTokenDirection::Outgoing(_) => 
                panic!("Direction of token channel is already Outgoing!"),
        };

        let outgoing_tc = OutgoingTokenChannel::new(
            self.opt_token_channel.take().expect("token channel not present!"));

        let mut tc_sender = TokenChannelSender::new(outgoing_tc);
        f(&mut tc_sender);
        let TokenChannelSender {outgoing_tc} = tc_sender;

        let (token_channel, operations) = outgoing_tc.commit();
        self.opt_token_channel = Some(token_channel);

        let neighbor_move_token_inner = NeighborMoveTokenInner {
            operations,
            old_token: self.new_token.clone(),
            rand_nonce: rand_nonce.clone(),
        };

        self.new_token = calc_channel_next_token(self.token_channel_index,
                                &neighbor_move_token_inner);
        self.direction = MoveTokenDirection::Outgoing(
            neighbor_move_token_inner);
    }
    */

    #[allow(unused)]
    pub fn get_outgoing_move_token(&self) -> Option<NeighborMoveToken> {
        match self.direction {
            MoveTokenDirection::Incoming => None,
            MoveTokenDirection::Outgoing(ref move_token_inner) => {
                Some(NeighborMoveToken {
                    token_channel_index: self.token_channel_index,
                    operations: move_token_inner.operations.clone(),
                    old_token: move_token_inner.old_token.clone(),
                    rand_nonce: move_token_inner.rand_nonce.clone(),
                    new_token: self.new_token.clone(),
                })
            }
        }
    }
}
