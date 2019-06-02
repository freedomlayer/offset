use std::io;

use crate::capnp_common::{
    read_commit, read_custom_int128, read_custom_u_int128, read_invoice_id, read_multi_commit,
    read_named_index_server_address, read_named_relay_address, read_payment_id, read_public_key,
    read_rate, read_receipt, read_relay_address, read_signature, read_uid, write_commit,
    write_custom_int128, write_custom_u_int128, write_invoice_id, write_multi_commit,
    write_named_index_server_address, write_named_relay_address, write_payment_id,
    write_public_key, write_rate, write_receipt, write_relay_address, write_signature, write_uid,
};
use capnp;
use capnp::serialize_packed;
use common::int_convert::usize_to_u32;

use crate::serialize::SerializeError;
use app_server_capnp;

use crate::index_client::messages::{ClientResponseRoutes, ResponseRoutesResult};

use crate::report::serialize::{
    deser_node_report, deser_node_report_mutation, ser_node_report, ser_node_report_mutation,
};
use index_server::serialize::{
    deser_multi_route, deser_request_routes, ser_multi_route, ser_request_routes,
};

use crate::funder::messages::{
    AckClosePayment, AddFriend, AddInvoice, CreatePayment, CreateTransaction, PaymentStatus,
    ReceiptAck, RequestResult, ResetFriendChannel, ResponseClosePayment, SetFriendName,
    SetFriendRate, SetFriendRelays, SetFriendRemoteMaxDebt, TransactionResult,
    UserRequestSendFunds,
};
use crate::funder::serialize::{deser_friends_route, ser_friends_route};

use crate::app_server::messages::{
    AppPermissions, AppRequest, AppServerToApp, AppToAppServer, ReportMutations,
};

fn ser_receipt_ack(
    receipt_ack: &ReceiptAck,
    receipt_ack_builder: &mut app_server_capnp::receipt_ack::Builder,
) {
    write_uid(
        &receipt_ack.request_id,
        &mut receipt_ack_builder.reborrow().init_request_id(),
    );
    write_signature(
        &receipt_ack.receipt_signature,
        &mut receipt_ack_builder.reborrow().init_receipt_signature(),
    );
}

fn deser_receipt_ack(
    receipt_ack_reader: &app_server_capnp::receipt_ack::Reader,
) -> Result<ReceiptAck, SerializeError> {
    Ok(ReceiptAck {
        request_id: read_uid(&receipt_ack_reader.get_request_id()?)?,
        receipt_signature: read_signature(&receipt_ack_reader.get_receipt_signature()?)?,
    })
}

fn ser_add_friend(
    add_friend: &AddFriend,
    add_friend_builder: &mut app_server_capnp::add_friend::Builder,
) {
    write_public_key(
        &add_friend.friend_public_key,
        &mut add_friend_builder.reborrow().init_friend_public_key(),
    );

    let relays_len = usize_to_u32(add_friend.relays.len()).unwrap();
    let mut relays_builder = add_friend_builder.reborrow().init_relays(relays_len);
    for (index, relay_address) in add_friend.relays.iter().enumerate() {
        let mut relay_address_builder = relays_builder.reborrow().get(usize_to_u32(index).unwrap());
        write_relay_address(relay_address, &mut relay_address_builder);
    }

    add_friend_builder.reborrow().set_name(&add_friend.name);
    write_custom_int128(
        add_friend.balance,
        &mut add_friend_builder.reborrow().init_balance(),
    );
}

fn deser_add_friend(
    add_friend_reader: &app_server_capnp::add_friend::Reader,
) -> Result<AddFriend, SerializeError> {
    // TODO
    let mut relays = Vec::new();
    for relay_address in add_friend_reader.get_relays()? {
        relays.push(read_relay_address(&relay_address)?);
    }

    Ok(AddFriend {
        friend_public_key: read_public_key(&add_friend_reader.get_friend_public_key()?)?,
        relays,
        name: add_friend_reader.get_name()?.to_owned(),
        balance: read_custom_int128(&add_friend_reader.get_balance()?)?,
    })
}

fn ser_set_friend_name(
    set_friend_name: &SetFriendName,
    set_friend_name_builder: &mut app_server_capnp::set_friend_name::Builder,
) {
    write_public_key(
        &set_friend_name.friend_public_key,
        &mut set_friend_name_builder.reborrow().init_friend_public_key(),
    );

    set_friend_name_builder.set_name(&set_friend_name.name);
}

fn deser_set_friend_name(
    set_friend_name_reader: &app_server_capnp::set_friend_name::Reader,
) -> Result<SetFriendName, SerializeError> {
    Ok(SetFriendName {
        friend_public_key: read_public_key(&set_friend_name_reader.get_friend_public_key()?)?,
        name: set_friend_name_reader.get_name()?.to_owned(),
    })
}

fn ser_set_friend_relays(
    set_friend_relays: &SetFriendRelays,
    set_friend_relays_builder: &mut app_server_capnp::set_friend_relays::Builder,
) {
    write_public_key(
        &set_friend_relays.friend_public_key,
        &mut set_friend_relays_builder
            .reborrow()
            .init_friend_public_key(),
    );

    let relays_len = usize_to_u32(set_friend_relays.relays.len()).unwrap();
    let mut relays_builder = set_friend_relays_builder.reborrow().init_relays(relays_len);
    for (index, relay_address) in set_friend_relays.relays.iter().enumerate() {
        let mut relay_address_builder = relays_builder.reborrow().get(usize_to_u32(index).unwrap());
        write_relay_address(relay_address, &mut relay_address_builder);
    }
}

fn deser_set_friend_relays(
    set_friend_relays_reader: &app_server_capnp::set_friend_relays::Reader,
) -> Result<SetFriendRelays, SerializeError> {
    let mut relays = Vec::new();
    for relay_address in set_friend_relays_reader.get_relays()? {
        relays.push(read_relay_address(&relay_address)?);
    }

    Ok(SetFriendRelays {
        friend_public_key: read_public_key(&set_friend_relays_reader.get_friend_public_key()?)?,
        relays,
    })
}

fn ser_set_friend_remote_max_debt(
    set_friend_remote_max_debt: &SetFriendRemoteMaxDebt,
    set_friend_remote_max_debt_builder: &mut app_server_capnp::set_friend_remote_max_debt::Builder,
) {
    write_public_key(
        &set_friend_remote_max_debt.friend_public_key,
        &mut set_friend_remote_max_debt_builder
            .reborrow()
            .init_friend_public_key(),
    );

    write_custom_u_int128(
        set_friend_remote_max_debt.remote_max_debt,
        &mut set_friend_remote_max_debt_builder
            .reborrow()
            .init_remote_max_debt(),
    );
}

fn deser_set_friend_remote_max_debt(
    set_friend_remote_max_debt_reader: &app_server_capnp::set_friend_remote_max_debt::Reader,
) -> Result<SetFriendRemoteMaxDebt, SerializeError> {
    Ok(SetFriendRemoteMaxDebt {
        friend_public_key: read_public_key(
            &set_friend_remote_max_debt_reader.get_friend_public_key()?,
        )?,
        remote_max_debt: read_custom_u_int128(
            &set_friend_remote_max_debt_reader.get_remote_max_debt()?,
        )?,
    })
}

fn ser_reset_friend_channel(
    reset_friend_channel: &ResetFriendChannel,
    reset_friend_channel_builder: &mut app_server_capnp::reset_friend_channel::Builder,
) {
    write_public_key(
        &reset_friend_channel.friend_public_key,
        &mut reset_friend_channel_builder
            .reborrow()
            .init_friend_public_key(),
    );

    write_signature(
        &reset_friend_channel.reset_token,
        &mut reset_friend_channel_builder.reborrow().init_reset_token(),
    );
}

fn deser_reset_friend_channel(
    reset_friend_channel_reader: &app_server_capnp::reset_friend_channel::Reader,
) -> Result<ResetFriendChannel, SerializeError> {
    Ok(ResetFriendChannel {
        friend_public_key: read_public_key(&reset_friend_channel_reader.get_friend_public_key()?)?,
        reset_token: read_signature(&reset_friend_channel_reader.get_reset_token()?)?,
    })
}

// TODO: Add serialization code for ResponseRoutesResult, ClientResponseRoutes
fn ser_response_routes_result(
    response_routes_result: &ResponseRoutesResult,
    response_routes_result_builder: &mut app_server_capnp::response_routes_result::Builder,
) {
    match response_routes_result {
        ResponseRoutesResult::Success(multi_routes) => {
            let multi_routes_len = usize_to_u32(multi_routes.len()).unwrap();
            let mut multi_routes_builder = response_routes_result_builder
                .reborrow()
                .init_success(multi_routes_len);
            for (index, multi_route) in multi_routes.iter().enumerate() {
                let mut multi_route_builder = multi_routes_builder
                    .reborrow()
                    .get(usize_to_u32(index).unwrap());
                ser_multi_route(multi_route, &mut multi_route_builder);
            }
        }
        ResponseRoutesResult::Failure => response_routes_result_builder.reborrow().set_failure(()),
    }
}

fn deser_response_routes_result(
    response_routes_result_reader: &app_server_capnp::response_routes_result::Reader,
) -> Result<ResponseRoutesResult, SerializeError> {
    Ok(match response_routes_result_reader.which()? {
        app_server_capnp::response_routes_result::Success(multi_routes_reader) => {
            let mut multi_routes = Vec::new();
            for multi_route_reader in multi_routes_reader? {
                multi_routes.push(deser_multi_route(&multi_route_reader)?);
            }
            ResponseRoutesResult::Success(multi_routes)
        }
        app_server_capnp::response_routes_result::Failure(()) => ResponseRoutesResult::Failure,
    })
}

fn ser_client_response_routes(
    client_response_routes: &ClientResponseRoutes,
    client_response_routes_builder: &mut app_server_capnp::client_response_routes::Builder,
) {
    write_uid(
        &client_response_routes.request_id,
        &mut client_response_routes_builder.reborrow().init_request_id(),
    );
    ser_response_routes_result(
        &client_response_routes.result,
        &mut client_response_routes_builder.reborrow().init_result(),
    );
}

fn deser_client_response_routes(
    client_response_routes_reader: &app_server_capnp::client_response_routes::Reader,
) -> Result<ClientResponseRoutes, SerializeError> {
    Ok(ClientResponseRoutes {
        request_id: read_uid(&client_response_routes_reader.get_request_id()?)?,
        result: deser_response_routes_result(&client_response_routes_reader.get_result()?)?,
    })
}

/*
fn ser_add_index_server(add_index_server: &AddIndexServer<NetAddress>,
                            add_index_server_builder: &mut app_server_capnp::add_index_server::Builder) {

    write_public_key(&add_index_server.public_key, &mut add_index_server_builder.reborrow().init_public_key());
    write_net_address(&add_index_server.address,&mut add_index_server_builder.reborrow().init_address());
    add_index_server_builder.reborrow().set_name(&add_index_server.name);
}

fn deser_add_index_server(add_index_server_reader: &app_server_capnp::add_index_server::Reader)
    -> Result<AddIndexServer<NetAddress>, SerializeError> {

    Ok(AddIndexServer {
        public_key: read_public_key(&add_index_server_reader.get_public_key()?)?,
        address: read_net_address(&add_index_server_reader.get_address()?)?,
        name: add_index_server_reader.get_name()?.to_owned(),
    })
}
*/

fn ser_app_permissions(
    app_permissions: &AppPermissions,
    app_permissions_builder: &mut app_server_capnp::app_permissions::Builder,
) {
    app_permissions_builder
        .reborrow()
        .set_routes(app_permissions.routes);
    app_permissions_builder
        .reborrow()
        .set_buyer(app_permissions.buyer);
    app_permissions_builder
        .reborrow()
        .set_seller(app_permissions.seller);
    app_permissions_builder
        .reborrow()
        .set_config(app_permissions.config);
}

fn deser_app_permissions(
    app_permissions_reader: &app_server_capnp::app_permissions::Reader,
) -> Result<AppPermissions, SerializeError> {
    Ok(AppPermissions {
        routes: app_permissions_reader.get_routes(),
        buyer: app_permissions_reader.get_buyer(),
        seller: app_permissions_reader.get_seller(),
        config: app_permissions_reader.get_config(),
    })
}

fn ser_create_payment(
    create_payment: &CreatePayment,
    create_payment_builder: &mut app_server_capnp::create_payment::Builder,
) {
    write_payment_id(
        &create_payment.payment_id,
        &mut create_payment_builder.reborrow().init_payment_id(),
    );

    write_invoice_id(
        &create_payment.invoice_id,
        &mut create_payment_builder.reborrow().init_invoice_id(),
    );

    write_custom_u_int128(
        create_payment.total_dest_payment,
        &mut create_payment_builder.reborrow().init_total_dest_payment(),
    );

    write_public_key(
        &create_payment.dest_public_key,
        &mut create_payment_builder.reborrow().init_dest_public_key(),
    );
}

fn deser_create_payment(
    create_payment_reader: &app_server_capnp::create_payment::Reader,
) -> Result<CreatePayment, SerializeError> {
    Ok(CreatePayment {
        payment_id: read_payment_id(&create_payment_reader.get_payment_id()?)?,
        invoice_id: read_invoice_id(&create_payment_reader.get_invoice_id()?)?,
        total_dest_payment: read_custom_u_int128(&create_payment_reader.get_total_dest_payment()?)?,
        dest_public_key: read_public_key(&create_payment_reader.get_dest_public_key()?)?,
    })
}

fn ser_create_transaction(
    create_transaction: &CreateTransaction,
    create_transaction_builder: &mut app_server_capnp::create_transaction::Builder,
) {
    write_payment_id(
        &create_transaction.payment_id,
        &mut create_transaction_builder.reborrow().init_payment_id(),
    );

    write_uid(
        &create_transaction.request_id,
        &mut create_transaction_builder.reborrow().init_request_id(),
    );

    ser_friends_route(
        &create_transaction.route,
        &mut create_transaction_builder.reborrow().init_route(),
    );

    write_custom_u_int128(
        create_transaction.dest_payment,
        &mut create_transaction_builder.reborrow().init_dest_payment(),
    );

    write_custom_u_int128(
        create_transaction.fees,
        &mut create_transaction_builder.reborrow().init_fees(),
    );
}

fn deser_create_transaction(
    create_transaction_reader: &app_server_capnp::create_transaction::Reader,
) -> Result<CreateTransaction, SerializeError> {
    Ok(CreateTransaction {
        payment_id: read_payment_id(&create_transaction_reader.get_payment_id()?)?,
        request_id: read_uid(&create_transaction_reader.get_request_id()?)?,
        route: deser_friends_route(&create_transaction_reader.get_route()?)?,
        dest_payment: read_custom_u_int128(&create_transaction_reader.get_dest_payment()?)?,
        fees: read_custom_u_int128(&create_transaction_reader.get_fees()?)?,
    })
}

fn ser_add_invoice(
    add_invoice: &AddInvoice,
    add_invoice_builder: &mut app_server_capnp::add_invoice::Builder,
) {
    write_invoice_id(
        &add_invoice.invoice_id,
        &mut add_invoice_builder.reborrow().init_invoice_id(),
    );

    write_custom_u_int128(
        add_invoice.total_dest_payment,
        &mut add_invoice_builder.reborrow().init_total_dest_payment(),
    );
}

fn deser_add_invoice(
    add_invoice_reader: &app_server_capnp::add_invoice::Reader,
) -> Result<AddInvoice, SerializeError> {
    Ok(AddInvoice {
        invoice_id: read_invoice_id(&add_invoice_reader.get_invoice_id()?)?,
        total_dest_payment: read_custom_u_int128(&add_invoice_reader.get_total_dest_payment()?)?,
    })
}

fn ser_ack_close_payment(
    ack_close_payment: &AckClosePayment,
    ack_close_payment_builder: &mut app_server_capnp::ack_close_payment::Builder,
) {
    write_payment_id(
        &ack_close_payment.payment_id,
        &mut ack_close_payment_builder.reborrow().init_payment_id(),
    );

    write_uid(
        &ack_close_payment.ack_uid,
        &mut ack_close_payment_builder.reborrow().init_ack_uid(),
    );
}

fn deser_ack_close_payment(
    ack_close_payment_reader: &app_server_capnp::ack_close_payment::Reader,
) -> Result<AckClosePayment, SerializeError> {
    Ok(AckClosePayment {
        payment_id: read_payment_id(&ack_close_payment_reader.get_payment_id()?)?,
        ack_uid: read_uid(&ack_close_payment_reader.get_ack_uid()?)?,
    })
}

fn ser_set_friend_rate(
    set_friend_rate: &SetFriendRate,
    set_friend_rate_builder: &mut app_server_capnp::set_friend_rate::Builder,
) {
    write_public_key(
        &set_friend_rate.friend_public_key,
        &mut set_friend_rate_builder.reborrow().init_friend_public_key(),
    );

    write_rate(
        &set_friend_rate.rate,
        &mut set_friend_rate_builder.reborrow().init_rate(),
    );
}

fn deser_set_friend_rate(
    set_friend_rate_reader: &app_server_capnp::set_friend_rate::Reader,
) -> Result<SetFriendRate, SerializeError> {
    Ok(SetFriendRate {
        friend_public_key: read_public_key(&set_friend_rate_reader.get_friend_public_key()?)?,
        rate: read_rate(&set_friend_rate_reader.get_rate()?)?,
    })
}

fn ser_request_result(
    request_result: &RequestResult,
    request_result_builder: &mut app_server_capnp::request_result::Builder,
) {
    match request_result {
        RequestResult::Success(commit) => {
            write_commit(
                commit,
                &mut request_result_builder.reborrow().init_success(),
            );
        }
        RequestResult::Failure => request_result_builder.reborrow().set_failure(()),
    }
}

fn deser_request_result(
    request_result_reader: &app_server_capnp::request_result::Reader,
) -> Result<RequestResult, SerializeError> {
    Ok(match request_result_reader.which()? {
        app_server_capnp::request_result::Success(commit_reader) => {
            RequestResult::Success(read_commit(&commit_reader?)?)
        }
        app_server_capnp::request_result::Failure(()) => RequestResult::Failure,
    })
}

fn ser_transaction_result(
    transaction_result: &TransactionResult,
    transaction_result_builder: &mut app_server_capnp::transaction_result::Builder,
) {
    write_uid(
        &transaction_result.request_id,
        &mut transaction_result_builder.reborrow().init_request_id(),
    );

    ser_request_result(
        &transaction_result.result,
        &mut transaction_result_builder.reborrow().init_result(),
    );
}

fn deser_transaction_result(
    transaction_result_reader: &app_server_capnp::transaction_result::Reader,
) -> Result<TransactionResult, SerializeError> {
    Ok(TransactionResult {
        request_id: read_uid(&transaction_result_reader.get_request_id()?)?,
        result: deser_request_result(&transaction_result_reader.get_result()?)?,
    })
}

fn ser_payment_status(
    payment_status: &PaymentStatus,
    payment_status_builder: &mut app_server_capnp::payment_status::Builder,
) {
    match payment_status {
        PaymentStatus::PaymentNotFound => {
            payment_status_builder.reborrow().set_payment_not_found(())
        }
        PaymentStatus::InProgress => payment_status_builder.reborrow().set_in_progress(()),
        PaymentStatus::Success((receipt, ack_uid)) => {
            let mut payment_success_builder = payment_status_builder.reborrow().init_success();
            write_receipt(
                receipt,
                &mut payment_success_builder.reborrow().init_receipt(),
            );
            write_uid(
                ack_uid,
                &mut payment_success_builder.reborrow().init_ack_uid(),
            );
        }
        PaymentStatus::Canceled(ack_uid) => {
            let mut ack_uid_builder = payment_status_builder.reborrow().init_canceled();
            write_uid(ack_uid, &mut ack_uid_builder);
        }
    }
}

fn deser_payment_status(
    payment_status_reader: &app_server_capnp::payment_status::Reader,
) -> Result<PaymentStatus, SerializeError> {
    Ok(match payment_status_reader.which()? {
        app_server_capnp::payment_status::PaymentNotFound(()) => PaymentStatus::PaymentNotFound,
        app_server_capnp::payment_status::InProgress(()) => PaymentStatus::InProgress,
        app_server_capnp::payment_status::Success(res_payment_success_reader) => {
            let payment_success_reader = res_payment_success_reader?;
            PaymentStatus::Success((
                read_receipt(&payment_success_reader.get_receipt()?)?,
                read_uid(&payment_success_reader.get_ack_uid()?)?,
            ))
        }
        app_server_capnp::payment_status::Canceled(ack_uid_reader) => {
            PaymentStatus::Canceled(read_uid(&ack_uid_reader?)?)
        }
    })
}

fn ser_response_close_payment(
    response_close_payment: &ResponseClosePayment,
    response_close_payment_builder: &mut app_server_capnp::response_close_payment::Builder,
) {
    write_payment_id(
        &response_close_payment.payment_id,
        &mut response_close_payment_builder.reborrow().init_payment_id(),
    );

    ser_payment_status(
        &response_close_payment.status,
        &mut response_close_payment_builder.reborrow().init_status(),
    );
}

fn deser_response_close_payment(
    response_close_payment_reader: &app_server_capnp::response_close_payment::Reader,
) -> Result<ResponseClosePayment, SerializeError> {
    Ok(ResponseClosePayment {
        payment_id: read_payment_id(&response_close_payment_reader.get_payment_id()?)?,
        status: deser_payment_status(&response_close_payment_reader.get_status()?)?,
    })
}

fn ser_report_mutations(
    report_mutations: &ReportMutations,
    report_mutations_builder: &mut app_server_capnp::report_mutations::Builder,
) {
    let mut opt_app_request_id_builder = report_mutations_builder
        .reborrow()
        .init_opt_app_request_id();
    match report_mutations.opt_app_request_id {
        Some(app_request_id) => {
            let mut app_request_id_builder = opt_app_request_id_builder.init_app_request_id();
            write_uid(&app_request_id, &mut app_request_id_builder);
        }
        None => {
            opt_app_request_id_builder.reborrow().set_empty(());
        }
    };

    let mutations_len = usize_to_u32(report_mutations.mutations.len()).unwrap();
    let mut mutations_builder = report_mutations_builder
        .reborrow()
        .init_mutations(mutations_len);
    for (index, node_report_mutation) in report_mutations.mutations.iter().enumerate() {
        let mut node_report_mutation_builder = mutations_builder
            .reborrow()
            .get(usize_to_u32(index).unwrap());
        ser_node_report_mutation(node_report_mutation, &mut node_report_mutation_builder);
    }
}

fn deser_report_mutations(
    report_mutations_reader: &app_server_capnp::report_mutations::Reader,
) -> Result<ReportMutations, SerializeError> {
    let opt_app_request_id = match report_mutations_reader.get_opt_app_request_id().which()? {
        app_server_capnp::report_mutations::opt_app_request_id::AppRequestId(
            app_request_id_reader,
        ) => Some(read_uid(&app_request_id_reader?)?),
        app_server_capnp::report_mutations::opt_app_request_id::Empty(()) => None,
    };

    let mut mutations = Vec::new();
    for node_report_mutation in report_mutations_reader.get_mutations()? {
        mutations.push(deser_node_report_mutation(&node_report_mutation)?);
    }

    Ok(ReportMutations {
        opt_app_request_id,
        mutations,
    })
}

fn ser_app_server_to_app(
    app_server_to_app: &AppServerToApp,
    app_server_to_app_builder: &mut app_server_capnp::app_server_to_app::Builder,
) {
    match app_server_to_app {
        AppServerToApp::TransactionResult(transaction_result) => ser_transaction_result(
            transaction_result,
            &mut app_server_to_app_builder
                .reborrow()
                .init_transaction_result(),
        ),
        AppServerToApp::ResponseClosePayment(response_close_payment) => ser_response_close_payment(
            response_close_payment,
            &mut app_server_to_app_builder
                .reborrow()
                .init_response_close_payment(),
        ),
        AppServerToApp::Report(node_report) => ser_node_report(
            node_report,
            &mut app_server_to_app_builder.reborrow().init_report(),
        ),
        AppServerToApp::ReportMutations(report_mutations) => ser_report_mutations(
            report_mutations,
            &mut app_server_to_app_builder.reborrow().init_report_mutations(),
        ),
        AppServerToApp::ResponseRoutes(response_routes) => ser_client_response_routes(
            response_routes,
            &mut app_server_to_app_builder.reborrow().init_response_routes(),
        ),
    }
}

fn deser_app_server_to_app(
    app_server_to_app_reader: &app_server_capnp::app_server_to_app::Reader,
) -> Result<AppServerToApp, SerializeError> {
    Ok(match app_server_to_app_reader.which()? {
        app_server_capnp::app_server_to_app::TransactionResult(transaction_result_reader) => {
            AppServerToApp::TransactionResult(deser_transaction_result(
                &transaction_result_reader?,
            )?)
        }
        app_server_capnp::app_server_to_app::ResponseClosePayment(
            response_close_payment_reader,
        ) => AppServerToApp::ResponseClosePayment(deser_response_close_payment(
            &response_close_payment_reader?,
        )?),
        app_server_capnp::app_server_to_app::Report(node_report_reader) => {
            AppServerToApp::Report(deser_node_report(&node_report_reader?)?)
        }
        app_server_capnp::app_server_to_app::ReportMutations(report_mutations_reader) => {
            AppServerToApp::ReportMutations(deser_report_mutations(&report_mutations_reader?)?)
        }
        app_server_capnp::app_server_to_app::ResponseRoutes(client_response_routes_reader) => {
            AppServerToApp::ResponseRoutes(deser_client_response_routes(
                &client_response_routes_reader?,
            )?)
        }
    })
}

fn ser_app_request(
    app_request: &AppRequest,
    app_request_builder: &mut app_server_capnp::app_request::Builder,
) {
    match app_request {
        AppRequest::AddRelay(named_relay_address) => write_named_relay_address(
            named_relay_address,
            &mut app_request_builder.reborrow().init_add_relay(),
        ),
        AppRequest::RemoveRelay(public_key) => write_public_key(
            public_key,
            &mut app_request_builder.reborrow().init_remove_relay(),
        ),
        AppRequest::CreatePayment(create_payment) => ser_create_payment(
            create_payment,
            &mut app_request_builder.reborrow().init_create_payment(),
        ),
        AppRequest::CreateTransaction(create_transaction) => ser_create_transaction(
            create_transaction,
            &mut app_request_builder.reborrow().init_create_transaction(),
        ),
        AppRequest::RequestClosePayment(payment_id) => write_payment_id(
            payment_id,
            &mut app_request_builder.reborrow().init_request_close_payment(),
        ),
        AppRequest::AckClosePayment(ack_close_payment) => ser_ack_close_payment(
            ack_close_payment,
            &mut app_request_builder.reborrow().init_ack_close_payment(),
        ),
        AppRequest::AddInvoice(add_invoice) => ser_add_invoice(
            add_invoice,
            &mut app_request_builder.reborrow().init_add_invoice(),
        ),
        AppRequest::CancelInvoice(invoice_id) => write_invoice_id(
            invoice_id,
            &mut app_request_builder.reborrow().init_cancel_invoice(),
        ),
        AppRequest::CommitInvoice(multi_commit) => write_multi_commit(
            multi_commit,
            &mut app_request_builder.reborrow().init_commit_invoice(),
        ),
        AppRequest::AddFriend(add_friend) => ser_add_friend(
            add_friend,
            &mut app_request_builder.reborrow().init_add_friend(),
        ),
        AppRequest::SetFriendRelays(set_friend_relays) => ser_set_friend_relays(
            set_friend_relays,
            &mut app_request_builder.reborrow().init_set_friend_relays(),
        ),
        AppRequest::SetFriendName(set_friend_name) => ser_set_friend_name(
            set_friend_name,
            &mut app_request_builder.reborrow().init_set_friend_name(),
        ),
        AppRequest::RemoveFriend(friend_public_key) => write_public_key(
            friend_public_key,
            &mut app_request_builder.reborrow().init_remove_friend(),
        ),
        AppRequest::EnableFriend(friend_public_key) => write_public_key(
            friend_public_key,
            &mut app_request_builder.reborrow().init_enable_friend(),
        ),
        AppRequest::DisableFriend(friend_public_key) => write_public_key(
            friend_public_key,
            &mut app_request_builder.reborrow().init_disable_friend(),
        ),
        AppRequest::OpenFriend(friend_public_key) => write_public_key(
            friend_public_key,
            &mut app_request_builder.reborrow().init_open_friend(),
        ),
        AppRequest::CloseFriend(friend_public_key) => write_public_key(
            friend_public_key,
            &mut app_request_builder.reborrow().init_close_friend(),
        ),
        AppRequest::SetFriendRemoteMaxDebt(set_friend_remote_max_debt) => {
            ser_set_friend_remote_max_debt(
                set_friend_remote_max_debt,
                &mut app_request_builder
                    .reborrow()
                    .init_set_friend_remote_max_debt(),
            )
        }
        AppRequest::SetFriendRate(set_friend_rate) => ser_set_friend_rate(
            set_friend_rate,
            &mut app_request_builder.reborrow().init_set_friend_rate(),
        ),
        AppRequest::ResetFriendChannel(reset_friend_channel) => ser_reset_friend_channel(
            reset_friend_channel,
            &mut app_request_builder.reborrow().init_reset_friend_channel(),
        ),
        AppRequest::RequestRoutes(request_routes) => ser_request_routes(
            request_routes,
            &mut app_request_builder.reborrow().init_request_routes(),
        ),
        AppRequest::AddIndexServer(named_index_server_address_reader) => {
            write_named_index_server_address(
                named_index_server_address_reader,
                &mut app_request_builder.reborrow().init_add_index_server(),
            )
        }
        AppRequest::RemoveIndexServer(public_key) => write_public_key(
            public_key,
            &mut app_request_builder.reborrow().init_remove_index_server(),
        ),
    }
}

fn deser_app_request(
    app_request: &app_server_capnp::app_request::Reader,
) -> Result<AppRequest, SerializeError> {
    Ok(match app_request.which()? {
        app_server_capnp::app_request::AddRelay(named_relay_address_reader) => {
            AppRequest::AddRelay(read_named_relay_address(&named_relay_address_reader?)?)
        }
        app_server_capnp::app_request::RemoveRelay(public_key_reader) => {
            AppRequest::RemoveRelay(read_public_key(&public_key_reader?)?)
        }
        app_server_capnp::app_request::CreatePayment(create_payment_reader) => {
            AppRequest::CreatePayment(deser_create_payment(&create_payment_reader?)?)
        }
        app_server_capnp::app_request::CreateTransaction(create_transaction_reader) => {
            AppRequest::CreateTransaction(deser_create_transaction(&create_transaction_reader?)?)
        }
        app_server_capnp::app_request::RequestClosePayment(payment_id_reader) => {
            AppRequest::RequestClosePayment(read_payment_id(&payment_id_reader?)?)
        }
        app_server_capnp::app_request::AckClosePayment(ack_close_payment_reader) => {
            AppRequest::AckClosePayment(deser_ack_close_payment(&ack_close_payment_reader?)?)
        }
        app_server_capnp::app_request::AddInvoice(add_invoice_reader) => {
            AppRequest::AddInvoice(deser_add_invoice(&add_invoice_reader?)?)
        }
        app_server_capnp::app_request::CancelInvoice(invoice_id_reader) => {
            AppRequest::CancelInvoice(read_invoice_id(&invoice_id_reader?)?)
        }
        app_server_capnp::app_request::CommitInvoice(multi_commit_reader) => {
            AppRequest::CommitInvoice(read_multi_commit(&multi_commit_reader?)?)
        }
        app_server_capnp::app_request::AddFriend(add_friend_reader) => {
            AppRequest::AddFriend(deser_add_friend(&add_friend_reader?)?)
        }
        app_server_capnp::app_request::SetFriendRelays(set_friend_relays) => {
            AppRequest::SetFriendRelays(deser_set_friend_relays(&set_friend_relays?)?)
        }
        app_server_capnp::app_request::SetFriendName(set_friend_name) => {
            AppRequest::SetFriendName(deser_set_friend_name(&set_friend_name?)?)
        }
        app_server_capnp::app_request::RemoveFriend(public_key_reader) => {
            AppRequest::RemoveFriend(read_public_key(&public_key_reader?)?)
        }
        app_server_capnp::app_request::EnableFriend(public_key_reader) => {
            AppRequest::EnableFriend(read_public_key(&public_key_reader?)?)
        }
        app_server_capnp::app_request::DisableFriend(public_key_reader) => {
            AppRequest::DisableFriend(read_public_key(&public_key_reader?)?)
        }
        app_server_capnp::app_request::OpenFriend(public_key_reader) => {
            AppRequest::OpenFriend(read_public_key(&public_key_reader?)?)
        }
        app_server_capnp::app_request::CloseFriend(public_key_reader) => {
            AppRequest::CloseFriend(read_public_key(&public_key_reader?)?)
        }
        app_server_capnp::app_request::SetFriendRate(set_friend_rate_reader) => {
            AppRequest::SetFriendRate(deser_set_friend_rate(&set_friend_rate_reader?)?)
        }
        app_server_capnp::app_request::SetFriendRemoteMaxDebt(
            set_friend_remote_max_debt_reader,
        ) => AppRequest::SetFriendRemoteMaxDebt(deser_set_friend_remote_max_debt(
            &set_friend_remote_max_debt_reader?,
        )?),
        app_server_capnp::app_request::ResetFriendChannel(reset_friend_channel_reader) => {
            AppRequest::ResetFriendChannel(deser_reset_friend_channel(
                &reset_friend_channel_reader?,
            )?)
        }
        app_server_capnp::app_request::RequestRoutes(request_routes_reader) => {
            AppRequest::RequestRoutes(deser_request_routes(&request_routes_reader?)?)
        }
        app_server_capnp::app_request::AddIndexServer(add_index_server_reader) => {
            AppRequest::AddIndexServer(read_named_index_server_address(&add_index_server_reader?)?)
        }
        app_server_capnp::app_request::RemoveIndexServer(public_key_reader) => {
            AppRequest::RemoveIndexServer(read_public_key(&public_key_reader?)?)
        }
    })
}

fn ser_app_to_app_server(
    app_to_app_server: &AppToAppServer,
    app_to_app_server_builder: &mut app_server_capnp::app_to_app_server::Builder,
) {
    write_uid(
        &app_to_app_server.app_request_id,
        &mut app_to_app_server_builder.reborrow().init_app_request_id(),
    );

    ser_app_request(
        &app_to_app_server.app_request,
        &mut app_to_app_server_builder.reborrow().init_app_request(),
    );
}

fn deser_app_to_app_server(
    app_to_app_server_reader: &app_server_capnp::app_to_app_server::Reader,
) -> Result<AppToAppServer, SerializeError> {
    Ok(AppToAppServer {
        app_request_id: read_uid(&app_to_app_server_reader.get_app_request_id()?)?,
        app_request: deser_app_request(&app_to_app_server_reader.get_app_request()?)?,
    })
}

// ---------------------------------------------------
// ---------------------------------------------------
pub fn serialize_app_permissions(app_permissions: &AppPermissions) -> Vec<u8> {
    let mut builder = capnp::message::Builder::new_default();
    let mut app_permissions_builder =
        builder.init_root::<app_server_capnp::app_permissions::Builder>();
    ser_app_permissions(app_permissions, &mut app_permissions_builder);

    let mut ser_buff = Vec::new();
    serialize_packed::write_message(&mut ser_buff, &builder).unwrap();
    ser_buff
}

pub fn deserialize_app_permissions(data: &[u8]) -> Result<AppPermissions, SerializeError> {
    let mut cursor = io::Cursor::new(data);
    let reader =
        serialize_packed::read_message(&mut cursor, ::capnp::message::ReaderOptions::new())?;
    let app_permissions_reader = reader.get_root::<app_server_capnp::app_permissions::Reader>()?;

    deser_app_permissions(&app_permissions_reader)
}

pub fn serialize_app_server_to_app(app_server_to_app: &AppServerToApp) -> Vec<u8> {
    let mut builder = capnp::message::Builder::new_default();
    let mut app_server_to_app_builder =
        builder.init_root::<app_server_capnp::app_server_to_app::Builder>();
    ser_app_server_to_app(app_server_to_app, &mut app_server_to_app_builder);

    let mut ser_buff = Vec::new();
    serialize_packed::write_message(&mut ser_buff, &builder).unwrap();
    ser_buff
}

pub fn deserialize_app_server_to_app(data: &[u8]) -> Result<AppServerToApp, SerializeError> {
    let mut cursor = io::Cursor::new(data);
    let reader =
        serialize_packed::read_message(&mut cursor, ::capnp::message::ReaderOptions::new())?;
    let app_server_to_app_reader =
        reader.get_root::<app_server_capnp::app_server_to_app::Reader>()?;

    deser_app_server_to_app(&app_server_to_app_reader)
}

pub fn serialize_app_to_app_server(app_server_to_app: &AppToAppServer) -> Vec<u8> {
    let mut builder = capnp::message::Builder::new_default();
    let mut app_to_app_server = builder.init_root::<app_server_capnp::app_to_app_server::Builder>();
    ser_app_to_app_server(app_server_to_app, &mut app_to_app_server);

    let mut ser_buff = Vec::new();
    serialize_packed::write_message(&mut ser_buff, &builder).unwrap();
    ser_buff
}

pub fn deserialize_app_to_app_server(data: &[u8]) -> Result<AppToAppServer, SerializeError> {
    let mut cursor = io::Cursor::new(data);
    let reader =
        serialize_packed::read_message(&mut cursor, ::capnp::message::ReaderOptions::new())?;
    let app_to_app_server = reader.get_root::<app_server_capnp::app_to_app_server::Reader>()?;

    deser_app_to_app_server(&app_to_app_server)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_server::messages::{NodeReportMutation, RelayAddress};
    use crate::index_client::messages::IndexClientReportMutation;
    use crate::report::messages::FunderReportMutation;
    use crypto::identity::{PublicKey, PUBLIC_KEY_LEN};
    use crypto::uid::{Uid, UID_LEN};
    use std::convert::TryInto;

    #[test]
    fn test_serialize_app_permissions() {
        let app_permissions = AppPermissions {
            routes: false,
            buyer: true,
            seller: false,
            config: true,
        };

        let data = serialize_app_permissions(&app_permissions);
        let app_permissions2 = deserialize_app_permissions(&data).unwrap();
        assert_eq!(app_permissions, app_permissions2);
    }

    #[test]
    fn test_serialize_app_server_to_app() {
        let mut mutations = Vec::new();

        let funder_report_mutation =
            FunderReportMutation::RemoveFriend(PublicKey::from(&[0xaa; PUBLIC_KEY_LEN]));

        let index_client_report_mutation = IndexClientReportMutation::SetConnectedServer(Some(
            PublicKey::from(&[0xdd; PUBLIC_KEY_LEN]),
        ));

        mutations.push(NodeReportMutation::Funder(funder_report_mutation));
        mutations.push(NodeReportMutation::IndexClient(
            index_client_report_mutation,
        ));
        let report_mutations = ReportMutations {
            opt_app_request_id: Some(Uid::from(&[0; UID_LEN])),
            mutations,
        };
        let app_server_to_app = AppServerToApp::ReportMutations(report_mutations);

        let data = serialize_app_server_to_app(&app_server_to_app);
        let app_server_to_app2 = deserialize_app_server_to_app(&data).unwrap();
        assert_eq!(app_server_to_app, app_server_to_app2);
    }

    #[test]
    fn test_serialize_app_to_app_server() {
        let mut relays = Vec::new();
        relays.push(RelayAddress {
            public_key: PublicKey::from(&[0xaa; PUBLIC_KEY_LEN]),
            address: "MyAddress:1338".to_owned().try_into().unwrap(),
        });
        relays.push(RelayAddress {
            public_key: PublicKey::from(&[0xcc; PUBLIC_KEY_LEN]),
            address: "MyAddress:1339".to_owned().try_into().unwrap(),
        });

        let add_friend = AddFriend {
            friend_public_key: PublicKey::from(&[0xee; PUBLIC_KEY_LEN]),
            relays,
            name: "Friend name".to_owned(),
            balance: -500,
        };
        let app_to_app_server = AppToAppServer {
            app_request_id: Uid::from(&[1; UID_LEN]),
            app_request: AppRequest::AddFriend(add_friend),
        };

        let data = serialize_app_to_app_server(&app_to_app_server);
        let app_to_app_server2 = deserialize_app_to_app_server(&data).unwrap();
        assert_eq!(app_to_app_server, app_to_app_server2);
    }

    // TODO: More tests are required here
}
