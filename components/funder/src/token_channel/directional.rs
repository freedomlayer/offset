#![warn(unused)]

use std::convert::TryFrom;
use byteorder::{BigEndian, WriteBytesExt};

use crypto::identity::PublicKey;
use crypto::crypto_rand::{RandValue, RAND_VALUE_LEN};
use crypto::hash::sha_512_256;

use crate::consts::MAX_OPERATIONS_IN_BATCH;

use utils::int_convert::usize_to_u64;

use super::types::{TokenChannel, TcMutation};
use super::incoming::{ProcessOperationOutput, ProcessTransListError, 
    simulate_process_operations_list, IncomingMessage};
use super::outgoing::OutgoingTc;

use super::super::types::{FriendMoveToken, ChannelToken, 
    FriendMoveTokenRequest, ResetTerms};


// Prefix used for chain hashing of token channel fundss.
// NEXT is used for hashing for the next move token funds.
// RESET is used for resetting the token channel.
// The prefix allows the receiver to distinguish between the two cases.
const TOKEN_NEXT: &[u8] = b"NEXT";
const TOKEN_RESET: &[u8] = b"RESET";



/// Indicate the direction of the move token funds.
#[derive(Clone, Serialize, Deserialize)]
pub enum MoveTokenDirection {
    Incoming(ChannelToken), // new_token
    Outgoing(FriendMoveTokenRequest),
}

pub enum SetDirection {
    Incoming(ChannelToken), // new_token
    Outgoing(FriendMoveToken),
}

#[allow(unused)]
pub enum DirectionalMutation {
    TcMutation(TcMutation),
    SetDirection(SetDirection),
    SetTokenWanted,
}


#[derive(Clone, Serialize, Deserialize)]
pub struct DirectionalTc {
    pub direction: MoveTokenDirection,
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
    // In case of a reset, all the local pending requests will be canceled.
}


/// Calculate the next token channel, given values of previous FriendMoveToken funds.
fn calc_channel_next_token(move_token_funds: &FriendMoveToken) 
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


impl DirectionalTc {
    #[allow(unused)]
    pub fn new(local_public_key: &PublicKey, 
               remote_public_key: &PublicKey) -> DirectionalTc {

        let mut hash_buffer: Vec<u8> = Vec::new();

        let local_pk_hash = sha_512_256(local_public_key);
        let remote_pk_hash = sha_512_256(remote_public_key);
        let new_token_channel = TokenChannel::new(local_public_key, remote_public_key, 0);

        let rand_nonce = RandValue::try_from(&remote_pk_hash.as_ref()[.. RAND_VALUE_LEN]).unwrap();

        let first_move_token_lower = FriendMoveToken {
            operations: Vec::new(),
            old_token: ChannelToken::from(local_pk_hash.as_array_ref()),
            rand_nonce: rand_nonce.clone(),
        };

        // Calculate hash(FirstMoveTokenLower):
        let new_token = calc_channel_next_token(&first_move_token_lower);

        if local_pk_hash < remote_pk_hash {
            // We are the first sender
            let friend_move_token_request = FriendMoveTokenRequest {
                friend_move_token: FriendMoveToken {
                    operations: Vec::new(),
                    old_token: ChannelToken::from(local_pk_hash.as_array_ref()),
                    rand_nonce,
                },
                token_wanted: false,
            };
            DirectionalTc {
                direction: MoveTokenDirection::Outgoing(friend_move_token_request),
                token_channel: new_token_channel,
            }
        } else {
            // We are the second sender
            DirectionalTc {
                direction: MoveTokenDirection::Incoming(new_token),
                token_channel: new_token_channel,
            }
        }
    }

    pub fn new_from_remote_reset(local_public_key: &PublicKey, 
                      remote_public_key: &PublicKey, 
                      new_token: &ChannelToken, 
                      balance: i128) -> DirectionalTc {
        DirectionalTc {
            direction: MoveTokenDirection::Incoming(new_token.clone()),
            token_channel: TokenChannel::new(local_public_key, remote_public_key, balance),
        }
    }

    pub fn new_from_local_reset(local_public_key: &PublicKey, 
                      remote_public_key: &PublicKey, 
                      reset_move_token: &FriendMoveToken,
                      balance: i128) -> DirectionalTc {

        let friend_move_token_request = FriendMoveTokenRequest {
            friend_move_token: reset_move_token.clone(),
            token_wanted: false,
        };
        DirectionalTc {
            direction: MoveTokenDirection::Outgoing(friend_move_token_request),
            token_channel: TokenChannel::new(local_public_key, remote_public_key, balance),
        }
    }

    /// Get a reference to internal token_channel.
    pub fn get_token_channel(&self) -> &TokenChannel {
        &self.token_channel
    }

    #[allow(unused)]
    fn balance_for_reset(&self) -> i128 {
        self.get_token_channel().balance_for_reset()
    }

    pub fn remote_max_debt(&self) -> u128 {
        self.get_token_channel().state().balance.remote_max_debt
    }

    pub fn new_token(&self) -> ChannelToken {
        match &self.direction {
            MoveTokenDirection::Incoming(new_token) => new_token.clone(),
            MoveTokenDirection::Outgoing(outgoing_move_token) =>
                calc_channel_next_token(&outgoing_move_token.friend_move_token)
        }
    }

    #[allow(unused)]
    fn calc_channel_reset_token(&self) -> ChannelToken {
        calc_channel_reset_token(&self.new_token(),
                                 self.get_token_channel().balance_for_reset())
    }

    pub fn get_reset_terms(&self) -> ResetTerms {
        ResetTerms {
            reset_token: self.calc_channel_reset_token(),
            balance_for_reset: self.balance_for_reset(),
        }
    }

    pub fn is_outgoing(&self) -> bool {
        match self.direction {
            MoveTokenDirection::Incoming(_) => false,
            MoveTokenDirection::Outgoing(_) => true,
        }
    }

    pub fn mutate(&mut self, d_mutation: &DirectionalMutation) {
        match d_mutation {
            DirectionalMutation::TcMutation(tc_mutation) => {
                self.token_channel.mutate(tc_mutation);
            },
            DirectionalMutation::SetDirection(ref set_direction) => {
                self.direction = match set_direction {
                    SetDirection::Incoming(new_token) => MoveTokenDirection::Incoming(new_token.clone()),
                    SetDirection::Outgoing(friend_move_token) => {
                        MoveTokenDirection::Outgoing(FriendMoveTokenRequest {
                            friend_move_token: friend_move_token.clone(),
                            token_wanted: false,
                        })
                    }
                };
            },
            DirectionalMutation::SetTokenWanted => {
                match self.direction {
                    MoveTokenDirection::Incoming(_) => unreachable!(),
                    MoveTokenDirection::Outgoing(ref mut friend_move_token_request) => {
                        friend_move_token_request.token_wanted = true;
                    },
                }
            },
        }
    }


    #[allow(unused)]
    pub fn simulate_receive_move_token(&self, 
                              move_token_msg: FriendMoveToken)
        -> Result<ReceiveMoveTokenOutput, ReceiveMoveTokenError> {

        // Make sure that the given new_token is valid:
        let new_token = calc_channel_next_token(&move_token_msg);

        match &self.direction {
            MoveTokenDirection::Incoming(incoming_new_token) => {
                if new_token == *incoming_new_token {
                    // Duplicate
                    Ok(ReceiveMoveTokenOutput::Duplicate)
                } else {
                    // Inconsistency
                    Err(ReceiveMoveTokenError::ChainInconsistency)
                }
            },
            MoveTokenDirection::Outgoing(ref outgoing_move_token) => {
                let friend_move_token = &outgoing_move_token.friend_move_token;
                if move_token_msg.old_token == self.new_token() {
                    match simulate_process_operations_list(&self.token_channel,
                        move_token_msg.operations) {
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
                                DirectionalMutation::SetDirection(SetDirection::Incoming(new_token)));
                            Ok(ReceiveMoveTokenOutput::Received(move_token_received))
                        },
                        Err(e) => {
                            Err(ReceiveMoveTokenError::InvalidTransaction(e))
                        },
                    }
                } else if friend_move_token.old_token == new_token {
                    // We should retransmit our funds to the remote side.
                    Ok(ReceiveMoveTokenOutput::RetransmitOutgoing(friend_move_token.clone()))
                } else {
                    Err(ReceiveMoveTokenError::ChainInconsistency)
                }
            },
        }
    }

    #[allow(unused)]
    pub fn begin_outgoing_move_token(&self) -> Option<OutgoingTc> {
        if let MoveTokenDirection::Outgoing(_) = self.direction {
            return None;
        }

        Some(OutgoingTc::new(&self.token_channel, MAX_OPERATIONS_IN_BATCH))
    }


    #[allow(unused)]
    pub fn get_outgoing_move_token(&self) -> Option<FriendMoveTokenRequest> {
        match self.direction {
            MoveTokenDirection::Incoming(_)=> None,
            MoveTokenDirection::Outgoing(ref friend_move_token_request) => {
                Some(friend_move_token_request.clone())
            }
        }
    }
}
