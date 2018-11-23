#![allow(unused)]

use std::io;
use capnp;
use capnp::serialize_packed;
use crypto::identity::PublicKey;
use utils::int_convert::usize_to_u32;
use crate::capnp_common::{write_signature, read_signature,
                          write_custom_int128, read_custom_int128,
                          write_custom_u_int128, read_custom_u_int128,
                          write_rand_nonce, read_rand_nonce,
                          write_uid, read_uid,
                          write_invoice_id, read_invoice_id,
                          write_public_key, read_public_key};
use funder_capnp;

use super::messages::{FriendMessage, MoveTokenRequest, ResetTerms,
                    MoveToken, FriendTcOp, RequestSendFunds,
                    ResponseSendFunds, FailureSendFunds,
                    FriendsRoute, FreezeLink};


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

fn ser_friends_route(friend_route: &FriendsRoute,
                     friends_route_builder: &mut funder_capnp::friends_route::Builder) {
    unimplemented!();
}

fn ser_freeze_link(freeze_link: &FreezeLink,
                   freeze_link_builder: &mut funder_capnp::freeze_link::Builder) {
    unimplemented!();
}


fn ser_request_send_funds(request_send_funds: &RequestSendFunds,
                          request_send_funds_builder: &mut funder_capnp::request_send_funds_op::Builder) {
    write_uid(&request_send_funds.request_id, 
              &mut request_send_funds_builder.reborrow().init_request_id());

    let mut route_builder = request_send_funds_builder.reborrow().init_route();
    ser_friends_route(&request_send_funds.route, &mut route_builder);

    write_custom_u_int128(request_send_funds.dest_payment, 
              &mut request_send_funds_builder.reborrow().init_dest_payment());

    write_invoice_id(&request_send_funds.invoice_id, 
              &mut request_send_funds_builder.reborrow().init_invoice_id());

    let freeze_links_len = usize_to_u32(request_send_funds.freeze_links.len()).unwrap();
    let mut freeze_links_builder = request_send_funds_builder.reborrow().init_freeze_links(freeze_links_len);

    for (index, freeze_link) in request_send_funds.freeze_links.iter().enumerate() {
        let mut freeze_link_builder = freeze_links_builder.reborrow().get(usize_to_u32(index).unwrap());
        ser_freeze_link(freeze_link, &mut freeze_link_builder);
    }
}

fn ser_response_send_funds(response_send_funds: &ResponseSendFunds,
                          response_send_funds_builder: &mut funder_capnp::response_send_funds_op::Builder) {

    write_uid(&response_send_funds.request_id,
              &mut response_send_funds_builder.reborrow().init_request_id());
    write_rand_nonce(&response_send_funds.rand_nonce,
              &mut response_send_funds_builder.reborrow().init_rand_nonce());
    write_signature(&response_send_funds.signature,
              &mut response_send_funds_builder.reborrow().init_signature());
}

fn ser_failure_send_funds(failure_send_funds: &FailureSendFunds,
                          failure_send_funds_builder: &mut funder_capnp::failure_send_funds_op::Builder) {
    write_uid(&failure_send_funds.request_id,
              &mut failure_send_funds_builder.reborrow().init_request_id());
    write_public_key(&failure_send_funds.reporting_public_key,
              &mut failure_send_funds_builder.reborrow().init_reporting_public_key());
    write_rand_nonce(&failure_send_funds.rand_nonce,
              &mut failure_send_funds_builder.reborrow().init_rand_nonce());
    write_signature(&failure_send_funds.signature,
              &mut failure_send_funds_builder.reborrow().init_signature());
}

fn ser_operation(operation: &FriendTcOp,
                 operation_builder: &mut funder_capnp::friend_operation::Builder) {

    match operation {
        FriendTcOp::EnableRequests => operation_builder.set_enable_requests(()),
        FriendTcOp::DisableRequests => operation_builder.set_disable_requests(()),
        FriendTcOp::SetRemoteMaxDebt(remote_max_debt) => {
            let mut set_remote_max_debt_builder = operation_builder.reborrow().init_set_remote_max_debt();
            write_custom_u_int128(*remote_max_debt, &mut set_remote_max_debt_builder);
        },
        FriendTcOp::RequestSendFunds(request_send_funds) => {
            let mut request_send_funds_builder = operation_builder.reborrow().init_request_send_funds();
            ser_request_send_funds(request_send_funds, &mut request_send_funds_builder);
        },
        FriendTcOp::ResponseSendFunds(response_send_funds) => {
            let mut response_send_funds_builder = operation_builder.reborrow().init_response_send_funds();
            ser_response_send_funds(response_send_funds, &mut response_send_funds_builder);
        },
        FriendTcOp::FailureSendFunds(failure_send_funds) => {
            let mut failure_send_funds_builder = operation_builder.reborrow().init_failure_send_funds();
            ser_failure_send_funds(failure_send_funds, &mut failure_send_funds_builder);
        },
    };
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
