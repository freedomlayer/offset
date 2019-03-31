use std::io;

use crate::capnp_common::{
    read_custom_int128, read_custom_u_int128, read_invoice_id, read_named_index_server_address,
    read_named_relay_address, read_public_key, read_receipt, read_relay_address, read_signature,
    read_uid, write_custom_int128, write_custom_u_int128, write_invoice_id,
    write_named_index_server_address, write_named_relay_address, write_public_key, write_receipt,
    write_relay_address, write_signature, write_uid,
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
    deser_request_routes, deser_route_with_capacity, ser_request_routes, ser_route_with_capacity,
};

use crate::funder::messages::{
    AddFriend, ReceiptAck, ResetFriendChannel, ResponseReceived, ResponseSendFundsResult,
    SetFriendName, SetFriendRelays, SetFriendRemoteMaxDebt, UserRequestSendFunds,
};
use crate::funder::serialize::{deser_friends_route, ser_friends_route};

use crate::app_server::messages::{
    AppPermissions, AppRequest, AppServerToApp, AppToAppServer, ReportMutations,
};

fn ser_user_request_send_funds(
    user_request_send_funds: &UserRequestSendFunds,
    user_request_send_funds_builder: &mut app_server_capnp::user_request_send_funds::Builder,
) {
    write_uid(
        &user_request_send_funds.request_id,
        &mut user_request_send_funds_builder.reborrow().init_request_id(),
    );

    let mut route_builder = user_request_send_funds_builder.reborrow().init_route();
    ser_friends_route(&user_request_send_funds.route, &mut route_builder);

    write_custom_u_int128(
        user_request_send_funds.dest_payment,
        &mut user_request_send_funds_builder
            .reborrow()
            .init_dest_payment(),
    );

    write_invoice_id(
        &user_request_send_funds.invoice_id,
        &mut user_request_send_funds_builder.reborrow().init_invoice_id(),
    );
}

fn deser_user_request_send_funds(
    user_request_send_funds_reader: &app_server_capnp::user_request_send_funds::Reader,
) -> Result<UserRequestSendFunds, SerializeError> {
    Ok(UserRequestSendFunds {
        request_id: read_uid(&user_request_send_funds_reader.get_request_id()?)?,
        route: deser_friends_route(&user_request_send_funds_reader.get_route()?)?,
        dest_payment: read_custom_u_int128(&user_request_send_funds_reader.get_dest_payment()?)?,
        invoice_id: read_invoice_id(&user_request_send_funds_reader.get_invoice_id()?)?,
    })
}

fn ser_response_received(
    response_received: &ResponseReceived,
    response_received_builder: &mut app_server_capnp::response_received::Builder,
) {
    write_uid(
        &response_received.request_id,
        &mut response_received_builder.reborrow().init_request_id(),
    );

    let result_builder = response_received_builder.reborrow().init_result();
    match &response_received.result {
        ResponseSendFundsResult::Success(receipt) => {
            let mut success_builder = result_builder.init_success();
            write_receipt(receipt, &mut success_builder);
        }
        ResponseSendFundsResult::Failure(public_key) => {
            let mut failure_builder = result_builder.init_failure();
            write_public_key(public_key, &mut failure_builder);
        }
    };
}

fn deser_response_received(
    response_received_reader: &app_server_capnp::response_received::Reader,
) -> Result<ResponseReceived, SerializeError> {
    let result = match response_received_reader.get_result().which()? {
        app_server_capnp::response_received::result::Success(receipt_reader) => {
            let receipt_reader = receipt_reader?;
            ResponseSendFundsResult::Success(read_receipt(&receipt_reader)?)
        }
        app_server_capnp::response_received::result::Failure(public_key_reader) => {
            let public_key_reader = public_key_reader?;
            ResponseSendFundsResult::Failure(read_public_key(&public_key_reader)?)
        }
    };

    Ok(ResponseReceived {
        request_id: read_uid(&response_received_reader.get_request_id()?)?,
        result,
    })
}

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
        ResponseRoutesResult::Success(routes_with_capacity) => {
            let routes_len = usize_to_u32(routes_with_capacity.len()).unwrap();
            let mut routes_with_capacity_builder = response_routes_result_builder
                .reborrow()
                .init_success(routes_len);
            for (index, route_with_capacity) in routes_with_capacity.iter().enumerate() {
                let mut route_with_capacity_builder = routes_with_capacity_builder
                    .reborrow()
                    .get(usize_to_u32(index).unwrap());
                ser_route_with_capacity(route_with_capacity, &mut route_with_capacity_builder);
            }
        }
        ResponseRoutesResult::Failure => response_routes_result_builder.reborrow().set_failure(()),
    }
}

fn deser_response_routes_result(
    response_routes_result_reader: &app_server_capnp::response_routes_result::Reader,
) -> Result<ResponseRoutesResult, SerializeError> {
    Ok(match response_routes_result_reader.which()? {
        app_server_capnp::response_routes_result::Success(routes_with_capacity_reader) => {
            let mut routes_with_capacity = Vec::new();
            for route_with_capacity in routes_with_capacity_reader? {
                routes_with_capacity.push(deser_route_with_capacity(&route_with_capacity)?);
            }
            ResponseRoutesResult::Success(routes_with_capacity)
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
        .set_send_funds(app_permissions.send_funds);
    app_permissions_builder
        .reborrow()
        .set_config(app_permissions.config);
}

fn deser_app_permissions(
    app_permissions_reader: &app_server_capnp::app_permissions::Reader,
) -> Result<AppPermissions, SerializeError> {
    Ok(AppPermissions {
        routes: app_permissions_reader.get_routes(),
        send_funds: app_permissions_reader.get_send_funds(),
        config: app_permissions_reader.get_config(),
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
        AppServerToApp::ResponseReceived(response_received) => ser_response_received(
            response_received,
            &mut app_server_to_app_builder
                .reborrow()
                .init_response_received(),
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
        app_server_capnp::app_server_to_app::ResponseReceived(response_received_reader) => {
            AppServerToApp::ResponseReceived(deser_response_received(&response_received_reader?)?)
        }
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
        AppRequest::RequestSendFunds(user_request_send_funds) => ser_user_request_send_funds(
            user_request_send_funds,
            &mut app_request_builder.reborrow().init_request_send_funds(),
        ),
        AppRequest::ReceiptAck(receipt_ack) => ser_receipt_ack(
            receipt_ack,
            &mut app_request_builder.reborrow().init_receipt_ack(),
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
        app_server_capnp::app_request::RequestSendFunds(request_send_funds_reader) => {
            AppRequest::RequestSendFunds(deser_user_request_send_funds(
                &request_send_funds_reader?,
            )?)
        }
        app_server_capnp::app_request::ReceiptAck(receipt_ack_reader) => {
            AppRequest::ReceiptAck(deser_receipt_ack(&receipt_ack_reader?)?)
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
            send_funds: true,
            config: false,
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
