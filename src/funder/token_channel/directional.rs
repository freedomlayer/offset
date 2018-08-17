#![warn(unused)]

use std::convert::TryFrom;
use byteorder::{BigEndian, WriteBytesExt};

use proto::funder::ChannelToken;
use crypto::identity::PublicKey;
use crypto::rand_values::{RandValue, RAND_VALUE_LEN};
use crypto::hash::sha_512_256;

use utils::int_convert::usize_to_u64;

use super::types::{TokenChannel, FriendMoveTokenInner, TcMutation};
use super::incoming::{ProcessOperationOutput, ProcessTransListError, 
    simulate_process_operations_list, IncomingMessage};
use super::outgoing::{OutgoingTokenChannel};

use super::super::types::{FriendMoveToken};


// Prefix used for chain hashing of token channel fundss.
// NEXT is used for hashing for the next move token funds.
// RESET is used for resetting the token channel.
// The prefix allows the receiver to distinguish between the two cases.
const TOKEN_NEXT: &[u8] = b"NEXT";
const TOKEN_RESET: &[u8] = b"RESET";


/// Indicate the direction of the move token funds.
#[derive(Clone)]
pub enum MoveTokenDirection {
    Incoming,
    Outgoing(FriendMoveTokenInner),
}


pub enum DirectionalMutation {
    TcMutation(TcMutation),
    SetDirection(MoveTokenDirection),
    SetNewToken(ChannelToken),
}


#[derive(Clone)]
pub struct DirectionalTokenChannel {
    pub direction: MoveTokenDirection,
    pub new_token: ChannelToken,
    // Equals Sha512/256(FriendMoveToken)
    pub token_channel: TokenChannel,
}

#[derive(Debug)]
pub enum ReceiveMoveTokenError {
    ChainInconsistency,
    InvalidTransaction(ProcessTransListError),
}

pub struct MoveTokenReceived {
    pub incoming_messages: Vec<IncomingMessage>,
    pub mutations: Vec<DirectionalMutation>,
}

pub enum ReceiveMoveTokenOutput {
    Duplicate,
    RetransmitOutgoing(FriendMoveToken),
    Received(MoveTokenReceived),
}


/// Calculate the next token channel, given values of previous FriendMoveToken funds.
fn calc_channel_next_token(move_token_funds: &FriendMoveTokenInner) 
                        -> ChannelToken {

    let mut contents = Vec::new();
    contents.write_u64::<BigEndian>(
        usize_to_u64(move_token_funds.operations.len()).unwrap()).unwrap();
    for op in &move_token_funds.operations {
        contents.extend_from_slice(&op.to_bytes());
    }

    let mut hash_buffer = Vec::new();
    hash_buffer.extend_from_slice(&sha_512_256(TOKEN_NEXT));
    hash_buffer.extend_from_slice(&contents);
    hash_buffer.extend_from_slice(&move_token_funds.old_token);
    hash_buffer.extend_from_slice(&move_token_funds.rand_nonce);
    let hash_result = sha_512_256(&hash_buffer);
    ChannelToken::from(hash_result.as_array_ref())
}

/// Calculate the token to be used for resetting the channel.
#[allow(unused)]
pub fn calc_channel_reset_token(new_token: &ChannelToken,
                      balance_for_reset: i128) -> ChannelToken {

    let mut hash_buffer = Vec::new();
    hash_buffer.extend_from_slice(&sha_512_256(TOKEN_RESET));
    hash_buffer.extend_from_slice(&new_token);
    hash_buffer.write_i128::<BigEndian>(balance_for_reset).unwrap();
    let hash_result = sha_512_256(&hash_buffer);
    ChannelToken::from(hash_result.as_array_ref())
}


impl DirectionalTokenChannel {
    #[allow(unused)]
    pub fn new(local_public_key: &PublicKey, 
               remote_public_key: &PublicKey) -> DirectionalTokenChannel {

        let mut hash_buffer: Vec<u8> = Vec::new();

        let local_pk_hash = sha_512_256(local_public_key);
        let remote_pk_hash = sha_512_256(remote_public_key);
        let new_token_channel = TokenChannel::new(local_public_key, remote_public_key, 0);

        let rand_nonce = RandValue::try_from(&remote_pk_hash.as_ref()[.. RAND_VALUE_LEN])
                    .expect("Failed to trim a public key hash into the size of random value!");

        let first_move_token_lower = FriendMoveTokenInner {
            operations: Vec::new(),
            old_token: ChannelToken::from(local_pk_hash.as_array_ref()),
            rand_nonce: rand_nonce.clone(),
        };

        // Calculate hash(FirstMoveTokenLower):
        let new_token = calc_channel_next_token(&first_move_token_lower);

        if local_pk_hash < remote_pk_hash {
            // We are the first sender
            DirectionalTokenChannel {
                direction: MoveTokenDirection::Outgoing(FriendMoveTokenInner {
                    operations: Vec::new(),
                    old_token: ChannelToken::from(local_pk_hash.as_array_ref()),
                    rand_nonce,
                }),
                new_token,
                token_channel: new_token_channel,
            }
        } else {
            // We are the second sender
            DirectionalTokenChannel {
                direction: MoveTokenDirection::Incoming,
                new_token,
                token_channel: new_token_channel,
            }
        }
    }

    pub fn new_from_reset(local_public_key: &PublicKey, 
                      remote_public_key: &PublicKey, 
                      current_token: &ChannelToken, 
                      balance: i128) -> DirectionalTokenChannel {
        DirectionalTokenChannel {
            direction: MoveTokenDirection::Incoming,
            new_token: current_token.clone(),
            token_channel: TokenChannel::new(local_public_key, remote_public_key, balance),
        }
    }

    /// Get a reference to internal token_channel.
    pub fn get_token_channel(&self) -> &TokenChannel {
        &self.token_channel
    }

    #[allow(unused)]
    pub fn balance_for_reset(&self) -> i128 {
        self.get_token_channel().balance_for_reset()
    }

    pub fn remote_max_debt(&self) -> u128 {
        self.get_token_channel().state().balance.remote_max_debt
    }

    #[allow(unused)]
    pub fn calc_channel_reset_token(&self) -> ChannelToken {
        calc_channel_reset_token(&self.new_token,
                                 self.get_token_channel().balance_for_reset())
    }

    pub fn mutate(&mut self, d_mutation: &DirectionalMutation) {
        match d_mutation {
            DirectionalMutation::TcMutation(tc_mutation) => {
                self.token_channel.mutate(tc_mutation);
            },
            DirectionalMutation::SetDirection(ref move_token_direction) => {
                self.direction = move_token_direction.clone();
            }
            DirectionalMutation::SetNewToken(ref new_token) => {
                self.new_token = new_token.clone();
            },
        }
    }


    #[allow(unused)]
    pub fn simulate_receive_move_token(&self, 
                              move_token_funds: FriendMoveTokenInner, 
                              new_token: ChannelToken) 
        -> Result<ReceiveMoveTokenOutput, ReceiveMoveTokenError> {

        // Make sure that the given new_token is valid:
        let expected_new_token = calc_channel_next_token(&move_token_funds);
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
                if move_token_funds.old_token == self.new_token {
                    match simulate_process_operations_list(&self.token_channel,
                        move_token_funds.operations) {
                        Ok(outputs) => {
                            let mut move_token_received = MoveTokenReceived {
                                incoming_messages: Vec::new(),
                                mutations: Vec::new(),
                            };

                            for output in outputs {
                                let ProcessOperationOutput 
                                    {incoming_message, tc_mutations} = output;

                                if let Some(funds) = incoming_message {
                                    move_token_received.incoming_messages.push(funds);
                                }
                                for tc_mutation in tc_mutations {
                                    move_token_received.mutations.push(
                                        DirectionalMutation::TcMutation(tc_mutation));
                                }
                            }
                            move_token_received.mutations.push(
                                DirectionalMutation::SetDirection(MoveTokenDirection::Incoming));
                            move_token_received.mutations.push(
                                DirectionalMutation::SetNewToken(new_token));
                            Ok(ReceiveMoveTokenOutput::Received(move_token_received))
                        },
                        Err(e) => {
                            Err(ReceiveMoveTokenError::InvalidTransaction(e))
                        },
                    }
                } else if move_token_inner.old_token == new_token {
                    // We should retransmit our funds to the remote side.
                    let outgoing_move_token = self.create_outgoing_move_token(move_token_inner);
                    Ok(ReceiveMoveTokenOutput::RetransmitOutgoing(outgoing_move_token))
                } else {
                    Err(ReceiveMoveTokenError::ChainInconsistency)
                }
            },
        }
    }

    #[allow(unused)]
    pub fn begin_outgoing_move_token(&self, move_token_max_length: usize) -> Option<OutgoingTokenChannel> {
        if let MoveTokenDirection::Outgoing(_) = self.direction {
            return None;
        }

        Some(OutgoingTokenChannel::new(&self.token_channel, move_token_max_length))
    }

    fn create_outgoing_move_token(&self, 
                                  move_token_inner: &FriendMoveTokenInner) 
                                        -> FriendMoveToken {
        FriendMoveToken {
            operations: move_token_inner.operations.clone(),
            old_token: move_token_inner.old_token.clone(),
            rand_nonce: move_token_inner.rand_nonce.clone(),
            new_token: self.new_token.clone(),
        }
    }

    #[allow(unused)]
    pub fn get_outgoing_move_token(&self) -> Option<FriendMoveToken> {
        match self.direction {
            MoveTokenDirection::Incoming => None,
            MoveTokenDirection::Outgoing(ref move_token_inner) => {
                Some(self.create_outgoing_move_token(move_token_inner))
            }
        }
    }
}
