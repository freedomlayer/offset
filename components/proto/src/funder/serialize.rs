use crate::capnp_common::{
    read_custom_int128, read_custom_u_int128, read_hashed_lock, read_invoice_id, read_plain_lock,
    read_public_key, read_rand_nonce, read_relay_address, read_signature, read_uid,
    write_custom_int128, write_custom_u_int128, write_hashed_lock, write_invoice_id,
    write_plain_lock, write_public_key, write_rand_nonce, write_relay_address, write_signature,
    write_uid,
};
use capnp;
use capnp::serialize_packed;
use common::int_convert::usize_to_u32;
use std::io;

use funder_capnp;

use super::messages::{
    CancelSendFundsOp, CollectSendFundsOp, FriendMessage, FriendTcOp, FriendsRoute, MoveToken,
    MoveTokenRequest, RequestSendFundsOp, ResetTerms, ResponseSendFundsOp,
};

use crate::serialize::SerializeError;

pub fn ser_friends_route(
    friends_route: &FriendsRoute,
    friends_route_builder: &mut funder_capnp::friends_route::Builder,
) {
    let public_keys_len = usize_to_u32(friends_route.public_keys.len()).unwrap();
    let mut public_keys_builder = friends_route_builder
        .reborrow()
        .init_public_keys(public_keys_len);

    for (index, public_key) in friends_route.public_keys.iter().enumerate() {
        let mut public_key_builder = public_keys_builder
            .reborrow()
            .get(usize_to_u32(index).unwrap());
        write_public_key(public_key, &mut public_key_builder);
    }
}

fn ser_request_send_funds_op(
    request_send_funds: &RequestSendFundsOp,
    request_send_funds_op_builder: &mut funder_capnp::request_send_funds_op::Builder,
) {
    write_uid(
        &request_send_funds.request_id,
        &mut request_send_funds_op_builder.reborrow().init_request_id(),
    );

    write_hashed_lock(
        &request_send_funds.src_hashed_lock,
        &mut request_send_funds_op_builder
            .reborrow()
            .init_src_hashed_lock(),
    );

    let mut route_builder = request_send_funds_op_builder.reborrow().init_route();
    ser_friends_route(&request_send_funds.route, &mut route_builder);

    write_custom_u_int128(
        request_send_funds.dest_payment,
        &mut request_send_funds_op_builder.reborrow().init_dest_payment(),
    );

    write_custom_u_int128(
        request_send_funds.total_dest_payment,
        &mut request_send_funds_op_builder
            .reborrow()
            .init_total_dest_payment(),
    );

    write_invoice_id(
        &request_send_funds.invoice_id,
        &mut request_send_funds_op_builder.reborrow().init_invoice_id(),
    );

    write_custom_u_int128(
        request_send_funds.left_fees,
        &mut request_send_funds_op_builder.reborrow().init_left_fees(),
    );
}

fn ser_response_send_funds_op(
    response_send_funds: &ResponseSendFundsOp,
    response_send_funds_op_builder: &mut funder_capnp::response_send_funds_op::Builder,
) {
    write_uid(
        &response_send_funds.request_id,
        &mut response_send_funds_op_builder.reborrow().init_request_id(),
    );

    write_hashed_lock(
        &response_send_funds.dest_hashed_lock,
        &mut response_send_funds_op_builder
            .reborrow()
            .init_dest_hashed_lock(),
    );

    write_rand_nonce(
        &response_send_funds.rand_nonce,
        &mut response_send_funds_op_builder.reborrow().init_rand_nonce(),
    );
    write_signature(
        &response_send_funds.signature,
        &mut response_send_funds_op_builder.reborrow().init_signature(),
    );
}

fn ser_cancel_send_funds_op(
    cancel_send_funds: &CancelSendFundsOp,
    cancel_send_funds_op_builder: &mut funder_capnp::cancel_send_funds_op::Builder,
) {
    write_uid(
        &cancel_send_funds.request_id,
        &mut cancel_send_funds_op_builder.reborrow().init_request_id(),
    );
}

fn ser_collect_send_funds_op(
    collect_send_funds: &CollectSendFundsOp,
    collect_send_funds_op_builder: &mut funder_capnp::collect_send_funds_op::Builder,
) {
    write_uid(
        &collect_send_funds.request_id,
        &mut collect_send_funds_op_builder.reborrow().init_request_id(),
    );

    write_plain_lock(
        &collect_send_funds.src_plain_lock,
        &mut collect_send_funds_op_builder
            .reborrow()
            .init_src_plain_lock(),
    );

    write_plain_lock(
        &collect_send_funds.dest_plain_lock,
        &mut collect_send_funds_op_builder
            .reborrow()
            .init_dest_plain_lock(),
    );
}

fn ser_friend_operation(
    operation: &FriendTcOp,
    operation_builder: &mut funder_capnp::friend_operation::Builder,
) {
    match operation {
        FriendTcOp::EnableRequests => operation_builder.set_enable_requests(()),
        FriendTcOp::DisableRequests => operation_builder.set_disable_requests(()),
        FriendTcOp::SetRemoteMaxDebt(remote_max_debt) => {
            let mut set_remote_max_debt_builder =
                operation_builder.reborrow().init_set_remote_max_debt();
            write_custom_u_int128(*remote_max_debt, &mut set_remote_max_debt_builder);
        }
        FriendTcOp::RequestSendFunds(request_send_funds) => {
            let mut request_send_funds_builder =
                operation_builder.reborrow().init_request_send_funds();
            ser_request_send_funds_op(request_send_funds, &mut request_send_funds_builder);
        }
        FriendTcOp::ResponseSendFunds(response_send_funds) => {
            let mut response_send_funds_builder =
                operation_builder.reborrow().init_response_send_funds();
            ser_response_send_funds_op(response_send_funds, &mut response_send_funds_builder);
        }
        FriendTcOp::CancelSendFunds(cancel_send_funds) => {
            let mut cancel_send_funds_builder =
                operation_builder.reborrow().init_cancel_send_funds();
            ser_cancel_send_funds_op(cancel_send_funds, &mut cancel_send_funds_builder);
        }
        FriendTcOp::CollectSendFunds(collect_send_funds) => {
            let mut collect_send_funds_builder =
                operation_builder.reborrow().init_collect_send_funds();
            ser_collect_send_funds_op(collect_send_funds, &mut collect_send_funds_builder);
        }
    };
}

fn ser_move_token(
    move_token: &MoveToken,
    move_token_builder: &mut funder_capnp::move_token::Builder,
) {
    let operations_len = usize_to_u32(move_token.operations.len()).unwrap();
    let mut operations_builder = move_token_builder
        .reborrow()
        .init_operations(operations_len);
    for (index, operation) in move_token.operations.iter().enumerate() {
        let mut operation_builder = operations_builder
            .reborrow()
            .get(usize_to_u32(index).unwrap());
        ser_friend_operation(operation, &mut operation_builder);
    }

    let mut opt_local_relays_builder = move_token_builder.reborrow().init_opt_local_relays();
    match &move_token.opt_local_relays {
        Some(local_address) => {
            let local_address_len = usize_to_u32(local_address.len()).unwrap();
            let mut address_builder = opt_local_relays_builder.init_relays(local_address_len);
            for (index, relay_address) in local_address.iter().enumerate() {
                let mut relay_address_builder =
                    address_builder.reborrow().get(usize_to_u32(index).unwrap());
                write_relay_address(relay_address, &mut relay_address_builder);
            }
        }
        None => {
            opt_local_relays_builder.set_empty(());
        }
    }

    write_public_key(
        &move_token.local_public_key,
        &mut move_token_builder.reborrow().init_local_public_key(),
    );
    write_public_key(
        &move_token.remote_public_key,
        &mut move_token_builder.reborrow().init_remote_public_key(),
    );

    write_signature(
        &move_token.old_token,
        &mut move_token_builder.reborrow().init_old_token(),
    );
    move_token_builder
        .reborrow()
        .set_inconsistency_counter(move_token.inconsistency_counter);
    write_custom_u_int128(
        move_token.move_token_counter,
        &mut move_token_builder.reborrow().init_move_token_counter(),
    );
    write_custom_int128(
        move_token.balance,
        &mut move_token_builder.reborrow().init_balance(),
    );
    write_custom_u_int128(
        move_token.local_pending_debt,
        &mut move_token_builder.reborrow().init_local_pending_debt(),
    );
    write_custom_u_int128(
        move_token.remote_pending_debt,
        &mut move_token_builder.reborrow().init_remote_pending_debt(),
    );
    write_rand_nonce(
        &move_token.rand_nonce,
        &mut move_token_builder.reborrow().init_rand_nonce(),
    );
    write_signature(
        &move_token.new_token,
        &mut move_token_builder.reborrow().init_new_token(),
    );
}

fn ser_move_token_request(
    move_token_request: &MoveTokenRequest,
    mut move_token_request_builder: funder_capnp::move_token_request::Builder,
) {
    let mut move_token_builder = move_token_request_builder.reborrow().init_move_token();
    ser_move_token(
        &move_token_request.friend_move_token,
        &mut move_token_builder,
    );

    move_token_request_builder.set_token_wanted(move_token_request.token_wanted);
}

fn ser_inconsistency_error(
    reset_terms: &ResetTerms,
    inconsistency_error_builder: &mut funder_capnp::inconsistency_error::Builder,
) {
    let mut reset_token = inconsistency_error_builder.reborrow().init_reset_token();
    write_signature(&reset_terms.reset_token, &mut reset_token);

    inconsistency_error_builder.set_inconsistency_counter(reset_terms.inconsistency_counter);

    let mut balance_for_reset = inconsistency_error_builder
        .reborrow()
        .init_balance_for_reset();
    write_custom_int128(reset_terms.balance_for_reset, &mut balance_for_reset);
}

fn ser_friend_message(
    friend_message: &FriendMessage,
    friend_message_builder: &mut funder_capnp::friend_message::Builder,
) {
    match friend_message {
        FriendMessage::MoveTokenRequest(move_token_request) => {
            let move_token_request_builder =
                friend_message_builder.reborrow().init_move_token_request();
            ser_move_token_request(move_token_request, move_token_request_builder);
        }
        FriendMessage::InconsistencyError(inconsistency_error) => {
            let mut inconsistency_error_builder =
                friend_message_builder.reborrow().init_inconsistency_error();
            ser_inconsistency_error(inconsistency_error, &mut inconsistency_error_builder);
        }
    };
}

/// Serialize a FriendMessage into a vector of bytes
pub fn serialize_friend_message(friend_message: &FriendMessage) -> Vec<u8> {
    let mut builder = capnp::message::Builder::new_default();
    let mut friend_message_builder = builder.init_root::<funder_capnp::friend_message::Builder>();

    ser_friend_message(friend_message, &mut friend_message_builder);

    let mut ser_buff = Vec::new();
    serialize_packed::write_message(&mut ser_buff, &builder).unwrap();
    ser_buff
}

// ------------ Deserialization -----------------------
// ----------------------------------------------------

pub fn deser_friends_route(
    friends_route_reader: &funder_capnp::friends_route::Reader,
) -> Result<FriendsRoute, SerializeError> {
    let mut public_keys = Vec::new();
    for public_key_reader in friends_route_reader.get_public_keys()? {
        public_keys.push(read_public_key(&public_key_reader)?);
    }

    Ok(FriendsRoute { public_keys })
}

fn deser_request_send_funds_op(
    request_send_funds_op_reader: &funder_capnp::request_send_funds_op::Reader,
) -> Result<RequestSendFundsOp, SerializeError> {
    Ok(RequestSendFundsOp {
        request_id: read_uid(&request_send_funds_op_reader.get_request_id()?)?,
        src_hashed_lock: read_hashed_lock(&request_send_funds_op_reader.get_src_hashed_lock()?)?,
        route: deser_friends_route(&request_send_funds_op_reader.get_route()?)?,
        dest_payment: read_custom_u_int128(&request_send_funds_op_reader.get_dest_payment()?)?,
        total_dest_payment: read_custom_u_int128(
            &request_send_funds_op_reader.get_total_dest_payment()?,
        )?,
        invoice_id: read_invoice_id(&request_send_funds_op_reader.get_invoice_id()?)?,
        left_fees: read_custom_u_int128(&request_send_funds_op_reader.get_left_fees()?)?,
    })
}

fn deser_response_send_funds_op(
    response_send_funds_op_reader: &funder_capnp::response_send_funds_op::Reader,
) -> Result<ResponseSendFundsOp, SerializeError> {
    Ok(ResponseSendFundsOp {
        request_id: read_uid(&response_send_funds_op_reader.get_request_id()?)?,
        dest_hashed_lock: read_hashed_lock(&response_send_funds_op_reader.get_dest_hashed_lock()?)?,
        rand_nonce: read_rand_nonce(&response_send_funds_op_reader.get_rand_nonce()?)?,
        signature: read_signature(&response_send_funds_op_reader.get_signature()?)?,
    })
}

fn deser_cancel_send_funds_op(
    cancel_send_funds_op_reader: &funder_capnp::cancel_send_funds_op::Reader,
) -> Result<CancelSendFundsOp, SerializeError> {
    Ok(CancelSendFundsOp {
        request_id: read_uid(&cancel_send_funds_op_reader.get_request_id()?)?,
    })
}

fn deser_collect_send_funds_op(
    collect_send_funds_op_reader: &funder_capnp::collect_send_funds_op::Reader,
) -> Result<CollectSendFundsOp, SerializeError> {
    Ok(CollectSendFundsOp {
        request_id: read_uid(&collect_send_funds_op_reader.get_request_id()?)?,
        src_plain_lock: read_plain_lock(&collect_send_funds_op_reader.get_src_plain_lock()?)?,
        dest_plain_lock: read_plain_lock(&collect_send_funds_op_reader.get_dest_plain_lock()?)?,
    })
}

fn deser_friend_operation(
    friend_operation_reader: &funder_capnp::friend_operation::Reader,
) -> Result<FriendTcOp, SerializeError> {
    Ok(match friend_operation_reader.which()? {
        funder_capnp::friend_operation::EnableRequests(()) => FriendTcOp::EnableRequests,
        funder_capnp::friend_operation::DisableRequests(()) => FriendTcOp::DisableRequests,
        funder_capnp::friend_operation::SetRemoteMaxDebt(set_remote_max_debt_reader) => {
            FriendTcOp::SetRemoteMaxDebt(read_custom_u_int128(&set_remote_max_debt_reader?)?)
        }
        funder_capnp::friend_operation::RequestSendFunds(request_send_funds_reader) => {
            FriendTcOp::RequestSendFunds(deser_request_send_funds_op(&request_send_funds_reader?)?)
        }
        funder_capnp::friend_operation::ResponseSendFunds(response_send_funds_reader) => {
            FriendTcOp::ResponseSendFunds(deser_response_send_funds_op(
                &response_send_funds_reader?,
            )?)
        }
        funder_capnp::friend_operation::CancelSendFunds(cancel_send_funds_reader) => {
            FriendTcOp::CancelSendFunds(deser_cancel_send_funds_op(&cancel_send_funds_reader?)?)
        }
        funder_capnp::friend_operation::CollectSendFunds(collect_send_funds_reader) => {
            FriendTcOp::CollectSendFunds(deser_collect_send_funds_op(&collect_send_funds_reader?)?)
        }
    })
}

fn deser_move_token(
    move_token_reader: &funder_capnp::move_token::Reader,
) -> Result<MoveToken, SerializeError> {
    let mut operations: Vec<FriendTcOp> = Vec::new();
    for operation_reader in move_token_reader.get_operations()? {
        operations.push(deser_friend_operation(&operation_reader)?);
    }

    let opt_local_relays_reader = move_token_reader.get_opt_local_relays();
    let opt_local_relays = match opt_local_relays_reader.which()? {
        funder_capnp::move_token::opt_local_relays::Empty(()) => None,
        funder_capnp::move_token::opt_local_relays::Relays(relay_address_reader) => {
            let mut addresses = Vec::new();
            for address in relay_address_reader? {
                addresses.push(read_relay_address(&address)?);
            }
            Some(addresses)
        }
    };

    Ok(MoveToken {
        operations,
        opt_local_relays,
        old_token: read_signature(&move_token_reader.get_old_token()?)?,
        local_public_key: read_public_key(&move_token_reader.get_local_public_key()?)?,
        remote_public_key: read_public_key(&move_token_reader.get_remote_public_key()?)?,
        inconsistency_counter: move_token_reader.get_inconsistency_counter(),
        move_token_counter: read_custom_u_int128(&move_token_reader.get_move_token_counter()?)?,
        balance: read_custom_int128(&move_token_reader.get_balance()?)?,
        local_pending_debt: read_custom_u_int128(&move_token_reader.get_local_pending_debt()?)?,
        remote_pending_debt: read_custom_u_int128(&move_token_reader.get_remote_pending_debt()?)?,
        rand_nonce: read_rand_nonce(&move_token_reader.get_rand_nonce()?)?,
        new_token: read_signature(&move_token_reader.get_new_token()?)?,
    })
}

fn deser_move_token_request(
    move_token_request_reader: &funder_capnp::move_token_request::Reader,
) -> Result<MoveTokenRequest, SerializeError> {
    let move_token_reader = move_token_request_reader.get_move_token()?;
    let move_token = deser_move_token(&move_token_reader)?;

    Ok(MoveTokenRequest {
        friend_move_token: move_token,
        token_wanted: move_token_request_reader.get_token_wanted(),
    })
}

fn deser_inconsistency_error(
    inconsistency_error_reader: &funder_capnp::inconsistency_error::Reader,
) -> Result<ResetTerms, SerializeError> {
    Ok(ResetTerms {
        reset_token: read_signature(&inconsistency_error_reader.get_reset_token()?)?,
        inconsistency_counter: inconsistency_error_reader.get_inconsistency_counter(),
        balance_for_reset: read_custom_int128(
            &inconsistency_error_reader.get_balance_for_reset()?,
        )?,
    })
}

fn deser_friend_message(
    friend_message_reader: &funder_capnp::friend_message::Reader,
) -> Result<FriendMessage, SerializeError> {
    Ok(match friend_message_reader.which()? {
        funder_capnp::friend_message::MoveTokenRequest(move_token_request_reader) => {
            FriendMessage::MoveTokenRequest(deser_move_token_request(&move_token_request_reader?)?)
        }
        funder_capnp::friend_message::InconsistencyError(inconsistency_error_reader) => {
            FriendMessage::InconsistencyError(deser_inconsistency_error(
                &inconsistency_error_reader?,
            )?)
        }
    })
}

/// Deserialize FriendMessage from an array of bytes
pub fn deserialize_friend_message(data: &[u8]) -> Result<FriendMessage, SerializeError> {
    let mut cursor = io::Cursor::new(data);
    let reader =
        serialize_packed::read_message(&mut cursor, ::capnp::message::ReaderOptions::new())?;
    let friend_message_reader = reader.get_root::<funder_capnp::friend_message::Reader>()?;

    deser_friend_message(&friend_message_reader)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_server::messages::RelayAddress;
    use crypto::crypto_rand::{RandValue, RAND_VALUE_LEN};
    use crypto::hash_lock::{HashedLock, PlainLock, HASHED_LOCK_LEN, PLAIN_LOCK_LEN};
    use crypto::identity::{PublicKey, Signature, PUBLIC_KEY_LEN, SIGNATURE_LEN};
    use crypto::invoice_id::{InvoiceId, INVOICE_ID_LEN};
    use crypto::uid::{Uid, UID_LEN};
    use std::convert::TryInto;

    /// Create an example FriendMessage::MoveTokenRequest:
    fn create_move_token_request() -> FriendMessage {
        let route = FriendsRoute {
            public_keys: vec![
                PublicKey::from(&[0x5; PUBLIC_KEY_LEN]),
                PublicKey::from(&[0x6; PUBLIC_KEY_LEN]),
                PublicKey::from(&[0x7; PUBLIC_KEY_LEN]),
                PublicKey::from(&[0x8; PUBLIC_KEY_LEN]),
            ],
        };

        let request_send_funds = RequestSendFundsOp {
            request_id: Uid::from(&[22; UID_LEN]),
            src_hashed_lock: HashedLock::from(&[1u8; HASHED_LOCK_LEN]),
            route,
            dest_payment: 48,
            total_dest_payment: 60,
            invoice_id: InvoiceId::from(&[0x99; INVOICE_ID_LEN]),
            left_fees: 14,
        };
        let response_send_funds = ResponseSendFundsOp {
            request_id: Uid::from(&[10; UID_LEN]),
            dest_hashed_lock: HashedLock::from(&[2u8; HASHED_LOCK_LEN]),
            rand_nonce: RandValue::from(&[0xbb; RAND_VALUE_LEN]),
            signature: Signature::from(&[3; SIGNATURE_LEN]),
        };

        let cancel_send_funds = CancelSendFundsOp {
            request_id: Uid::from(&[10; UID_LEN]),
        };

        let collect_send_funds = CollectSendFundsOp {
            request_id: Uid::from(&[10; UID_LEN]),
            src_plain_lock: PlainLock::from(&[4u8; PLAIN_LOCK_LEN]),
            dest_plain_lock: PlainLock::from(&[5u8; PLAIN_LOCK_LEN]),
        };

        let operations = vec![
            FriendTcOp::EnableRequests,
            FriendTcOp::DisableRequests,
            FriendTcOp::SetRemoteMaxDebt(101),
            FriendTcOp::RequestSendFunds(request_send_funds),
            FriendTcOp::ResponseSendFunds(response_send_funds),
            FriendTcOp::CancelSendFunds(cancel_send_funds),
            FriendTcOp::CollectSendFunds(collect_send_funds),
        ];

        let relay_address4 = RelayAddress {
            public_key: PublicKey::from(&[0x11; PUBLIC_KEY_LEN]),
            address: "MyAddress:1337".to_owned().try_into().unwrap(),
        };

        let relay_address6 = RelayAddress {
            public_key: PublicKey::from(&[0x11; PUBLIC_KEY_LEN]),
            address: "MyAddress:1338".to_owned().try_into().unwrap(),
        };

        let move_token = MoveToken {
            operations,
            opt_local_relays: Some(vec![relay_address4, relay_address6]),
            old_token: Signature::from(&[0; SIGNATURE_LEN]),
            local_public_key: PublicKey::from(&[0xaa; PUBLIC_KEY_LEN]),
            remote_public_key: PublicKey::from(&[0xbb; PUBLIC_KEY_LEN]),
            inconsistency_counter: 2,
            move_token_counter: 18,
            balance: -5,
            local_pending_debt: 20,
            remote_pending_debt: 80,
            rand_nonce: RandValue::from(&[0xaa; RAND_VALUE_LEN]),
            new_token: Signature::from(&[1; SIGNATURE_LEN]),
        };
        let move_token_request = MoveTokenRequest {
            friend_move_token: move_token,
            token_wanted: true,
        };

        FriendMessage::MoveTokenRequest(move_token_request)
    }

    /// Create an example FriendMessage::InconsistencyError
    fn create_inconsistency_error() -> FriendMessage {
        let reset_terms = ResetTerms {
            reset_token: Signature::from(&[2; SIGNATURE_LEN]),
            inconsistency_counter: 9,
            balance_for_reset: 301,
        };
        FriendMessage::InconsistencyError(reset_terms)
    }

    #[test]
    fn test_serialize_friend_message_move_token_request() {
        let friend_message = create_move_token_request();
        let ser_buff = serialize_friend_message(&friend_message);
        let friend_message2 = deserialize_friend_message(&ser_buff).unwrap();
        assert_eq!(friend_message, friend_message2);
    }

    #[test]
    fn test_serialize_friend_message_inconsistency_error() {
        let friend_message = create_inconsistency_error();
        let ser_buff = serialize_friend_message(&friend_message);
        let friend_message2 = deserialize_friend_message(&ser_buff).unwrap();
        assert_eq!(friend_message, friend_message2);
    }
}
