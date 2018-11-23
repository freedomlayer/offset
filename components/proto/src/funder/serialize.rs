#![allow(unused)]

use std::io;
use capnp;
use capnp::serialize_packed;
use crypto::identity::PublicKey;
use crate::capnp_common::{write_signature, read_signature};
use funder_capnp;

use super::messages::{FriendMessage, MoveTokenRequest, ResetTerms};


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


fn ser_move_token_request<'a>(move_token_request: &'a MoveTokenRequest,
                          move_token_request_builder: funder_capnp::move_token_request::Builder<'a>) {
    unimplemented!();
}

fn ser_inconsistency_error<'a>(reset_terms: &'a ResetTerms,
                          mut inconsistency_error_builder: funder_capnp::inconsistency_error::Builder<'a>) {

    let mut reset_token = inconsistency_error_builder.reborrow().init_reset_token();
    write_signature(&reset_terms.reset_token, &mut reset_token);

    inconsistency_error_builder.set_inconsistency_counter(reset_terms.inconsistency_counter.clone());

    let mut balance_for_reset = inconsistency_error_builder.init_balance_for_reset();

    // TODO; Solve problem with signed/unsigend here (AsRef is not implemented for i128?)
    // write_custom_u_int128(&reset_terms.balance_for_reset, &mut balance_for_reset);
}


fn ser_friend_message<'a>(friend_message: &'a FriendMessage, 
                          friend_message_builder: funder_capnp::friend_message::Builder<'a>) {

    match friend_message {
        FriendMessage::MoveTokenRequest(move_token_request) => {
            let mut move_token_request_builder = friend_message_builder.init_move_token_request();
            ser_move_token_request(move_token_request, move_token_request_builder);
        },
        FriendMessage::InconsistencyError(inconsistency_error) => {
            let mut inconsistency_error_builder = friend_message_builder.init_inconsistency_error();
            ser_inconsistency_error(inconsistency_error, inconsistency_error_builder);
        },
    };
}

pub fn deserialize_friend_message(data: &[u8]) -> Result<FriendMessage, FunderDeserializeError> {
    unimplemented!();
}
