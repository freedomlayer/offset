#![allow(unused)]

use std::io;
use capnp;
use capnp::serialize_packed;
use crypto::identity::PublicKey;
use utils::int_convert::usize_to_u32;
use crate::capnp_common::{write_signature, read_signature,
                            write_custom_int128, read_custom_int128,
                          write_custom_u_int128, read_custom_u_int128,
                          write_rand_nonce, read_rand_nonce};
use funder_capnp;

use super::messages::{FriendMessage, MoveTokenRequest, ResetTerms,
                    MoveToken, FriendTcOp};


#[derive(Debug)]
pub enum FunderDeserializeError {
    CapnpError(capnp::Error),
    NotInSchema(capnp::NotInSchema),
    IoError(io::Error),
}

impl From<capnp::Error> for FunderDeserializeError {
    fn from(e: capnp::Error) -> FunderDeserializeError {
        FunderDeserializeError::CapnpError(e)
    }
}

impl From<io::Error> for FunderDeserializeError {
    fn from(e: io::Error) -> FunderDeserializeError {
        FunderDeserializeError::IoError(e)
    }
}

fn ser_operation(operation: &FriendTcOp,
                 operation_builder: &mut funder_capnp::friend_operation::Builder) {
    unimplemented!();
}

fn ser_move_token(move_token: &MoveToken,
                      move_token_builder: &mut funder_capnp::move_token::Builder) {

    let operations_len = usize_to_u32(move_token.operations.len()).unwrap();
    let mut operations_builder = move_token_builder.reborrow().init_operations(operations_len);
    for (index, operation) in move_token.operations.iter().enumerate() {
        let mut operation_builder = operations_builder.reborrow().get(usize_to_u32(index).unwrap());
        ser_operation(operation, &mut operation_builder);
    }

    write_signature(&move_token.old_token, &mut move_token_builder.reborrow().init_old_token());
    move_token_builder.reborrow().set_inconsistency_counter(move_token.inconsistency_counter);
    write_custom_u_int128(move_token.move_token_counter, &mut move_token_builder.reborrow().init_move_token_counter());
    write_custom_int128(move_token.balance, &mut move_token_builder.reborrow().init_balance());
    write_custom_u_int128(move_token.local_pending_debt, &mut move_token_builder.reborrow().init_local_pending_debt());
    write_custom_u_int128(move_token.remote_pending_debt, &mut move_token_builder.reborrow().init_remote_pending_debt());
    write_rand_nonce(&move_token.rand_nonce, &mut move_token_builder.reborrow().init_rand_nonce());
    write_signature(&move_token.new_token, &mut move_token_builder.reborrow().init_new_token());
}

fn ser_move_token_request(move_token_request: &MoveTokenRequest,
                          mut move_token_request_builder: funder_capnp::move_token_request::Builder) {

    let mut move_token_builder = move_token_request_builder.reborrow().init_move_token();
    ser_move_token(&move_token_request.friend_move_token, &mut move_token_builder);

    move_token_request_builder.set_token_wanted(move_token_request.token_wanted);
}

fn ser_inconsistency_error(reset_terms: &ResetTerms,
                          inconsistency_error_builder: &mut funder_capnp::inconsistency_error::Builder) {

    let mut reset_token = inconsistency_error_builder.reborrow().init_reset_token();
    write_signature(&reset_terms.reset_token, &mut reset_token);

    inconsistency_error_builder.set_inconsistency_counter(reset_terms.inconsistency_counter.clone());

    let mut balance_for_reset = inconsistency_error_builder.reborrow().init_balance_for_reset();
    write_custom_int128(reset_terms.balance_for_reset, &mut balance_for_reset);
}


fn ser_friend_message(friend_message: &FriendMessage, 
                          friend_message_builder: &mut funder_capnp::friend_message::Builder) {

    match friend_message {
        FriendMessage::MoveTokenRequest(move_token_request) => {
            let mut move_token_request_builder = friend_message_builder.reborrow().init_move_token_request();
            ser_move_token_request(move_token_request, move_token_request_builder);
        },
        FriendMessage::InconsistencyError(inconsistency_error) => {
            let mut inconsistency_error_builder = friend_message_builder.reborrow().init_inconsistency_error();
            ser_inconsistency_error(inconsistency_error, &mut inconsistency_error_builder);
        },
    };
}

pub fn deserialize_friend_message(data: &[u8]) -> Result<FriendMessage, FunderDeserializeError> {
    unimplemented!();
}
