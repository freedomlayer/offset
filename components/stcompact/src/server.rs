use futures::{future, stream, StreamExt, channel::mpsc, Sink, SinkExt};

use common::select_streams::select_streams;
use common::conn::{ConnPair, BoxStream};

#[allow(unused)]
use app::conn::{AppConnTuple, AppServerToApp, AppToAppServer, AppPermissions, buyer, config, routes, seller};
use app::report::NodeReport;

use crate::types::{FromUser, ToUser, UserRequest};

type ConnPairCompact = ConnPair<ToUser, FromUser>;

#[derive(Debug)]
enum CompactServerEvent {
    User(FromUser),
    UserClosed,
    Node(AppServerToApp),
    NodeClosed,
}

enum CompactServerError {
    AppSenderError,
    UserSenderError,
    ProtocolError,
    ReportMutationError,
}

#[allow(unused)]
struct CompactServerState {
    node_report: NodeReport,
}

impl CompactServerState {
    fn new(node_report: NodeReport) -> Self {
        Self {
            node_report,
        }
    }
}

#[allow(unused)]
// TODO: Should we check permissions here in the future?
// Permissions are already checked on the node side (offst-app-server). I don't want to have code duplication here for
// permissions.
async fn handle_user<AS>(from_user: FromUser, _app_permissions: &AppPermissions, 
    server_state: &mut CompactServerState, app_sender: &mut AS) 
    -> Result<(), CompactServerError>
where   
    AS: Sink<AppToAppServer> + Unpin
{
    let FromUser {
        user_request_id,
        user_request,
    } = from_user;

    match user_request {
        // ==================[Configuration]==============================
        UserRequest::AddRelay(named_relay_address) => {
            let app_request = config::add_relay(named_relay_address);
            let app_to_app_server = AppToAppServer {
                app_request_id: user_request_id,
                app_request,
            };
            app_sender.send(app_to_app_server).await.map_err(|_| CompactServerError::AppSenderError)?;
        },
        UserRequest::RemoveRelay(relay_public_key) => {
            let app_request = config::remove_relay(relay_public_key);
            let app_to_app_server = AppToAppServer {
                app_request_id: user_request_id,
                app_request,
            };
            app_sender.send(app_to_app_server).await.map_err(|_| CompactServerError::AppSenderError)?;
        }
        UserRequest::AddIndexServer(named_index_server_address) => {
            let app_request = config::add_index_server(named_index_server_address);
            let app_to_app_server = AppToAppServer {
                app_request_id: user_request_id,
                app_request,
            };
            app_sender.send(app_to_app_server).await.map_err(|_| CompactServerError::AppSenderError)?;
        },
        UserRequest::RemoveIndexServer(index_public_key) => {
            let app_request = config::remove_index_server(index_public_key);
            let app_to_app_server = AppToAppServer {
                app_request_id: user_request_id,
                app_request,
            };
            app_sender.send(app_to_app_server).await.map_err(|_| CompactServerError::AppSenderError)?;
        },
        UserRequest::AddFriend(add_friend) => {
            let app_request = config::add_friend(add_friend.friend_public_key, 
                add_friend.relays, 
                add_friend.name);
            let app_to_app_server = AppToAppServer {
                app_request_id: user_request_id,
                app_request,
            };
            app_sender.send(app_to_app_server).await.map_err(|_| CompactServerError::AppSenderError)?;
        },
        UserRequest::SetFriendRelays(set_friend_relays) => {
            let app_request = config::set_friend_relays(set_friend_relays.friend_public_key,
                set_friend_relays.relays);
            let app_to_app_server = AppToAppServer {
                app_request_id: user_request_id,
                app_request,
            };
            app_sender.send(app_to_app_server).await.map_err(|_| CompactServerError::AppSenderError)?;
        },
        UserRequest::SetFriendName(set_friend_name) => {
            let app_request = config::set_friend_name(set_friend_name.friend_public_key,
                set_friend_name.name);
            let app_to_app_server = AppToAppServer {
                app_request_id: user_request_id,
                app_request,
            };
            app_sender.send(app_to_app_server).await.map_err(|_| CompactServerError::AppSenderError)?;
        },
        UserRequest::RemoveFriend(friend_public_key) => {
            let app_request = config::remove_friend(friend_public_key);
            let app_to_app_server = AppToAppServer {
                app_request_id: user_request_id,
                app_request,
            };
            app_sender.send(app_to_app_server).await.map_err(|_| CompactServerError::AppSenderError)?;
        },
        UserRequest::EnableFriend(friend_public_key) => {
            let app_request = config::enable_friend(friend_public_key);
            let app_to_app_server = AppToAppServer {
                app_request_id: user_request_id,
                app_request,
            };
            app_sender.send(app_to_app_server).await.map_err(|_| CompactServerError::AppSenderError)?;
        },
        UserRequest::DisableFriend(friend_public_key) => {
            let app_request = config::disable_friend(friend_public_key);
            let app_to_app_server = AppToAppServer {
                app_request_id: user_request_id,
                app_request,
            };
            app_sender.send(app_to_app_server).await.map_err(|_| CompactServerError::AppSenderError)?;
        },
        UserRequest::OpenFriendCurrency(open_friend_currency) => {
            let app_request = config::open_friend_currency(open_friend_currency.friend_public_key, 
                open_friend_currency.currency);
            let app_to_app_server = AppToAppServer {
                app_request_id: user_request_id,
                app_request,
            };
            app_sender.send(app_to_app_server).await.map_err(|_| CompactServerError::AppSenderError)?;
        },
        UserRequest::CloseFriendCurrency(close_friend_currency) => {
            let app_request = config::close_friend_currency(close_friend_currency.friend_public_key, 
                close_friend_currency.currency);
            let app_to_app_server = AppToAppServer {
                app_request_id: user_request_id,
                app_request,
            };
            app_sender.send(app_to_app_server).await.map_err(|_| CompactServerError::AppSenderError)?;
        },
        UserRequest::SetFriendCurrencyMaxDebt(set_friend_currency_max_debt) => {
            let app_request = config::set_friend_currency_max_debt(
                set_friend_currency_max_debt.friend_public_key, 
                set_friend_currency_max_debt.currency, 
                set_friend_currency_max_debt.remote_max_debt);
            let app_to_app_server = AppToAppServer {
                app_request_id: user_request_id,
                app_request,
            };
            app_sender.send(app_to_app_server).await.map_err(|_| CompactServerError::AppSenderError)?;
        },
        UserRequest::SetFriendCurrencyRate(set_friend_currency_rate) => {
            let app_request = config::set_friend_currency_rate(
                set_friend_currency_rate.friend_public_key, 
                set_friend_currency_rate.currency, 
                set_friend_currency_rate.rate);
            let app_to_app_server = AppToAppServer {
                app_request_id: user_request_id,
                app_request,
            };
            app_sender.send(app_to_app_server).await.map_err(|_| CompactServerError::AppSenderError)?;
        },
        UserRequest::RemoveFriendCurrency(remove_friend_currency) => {
            let app_request = config::remove_friend_currency(
                remove_friend_currency.friend_public_key, 
                remove_friend_currency.currency);
            let app_to_app_server = AppToAppServer {
                app_request_id: user_request_id,
                app_request,
            };
            app_sender.send(app_to_app_server).await.map_err(|_| CompactServerError::AppSenderError)?;
        },
        UserRequest::ResetFriendChannel(reset_friend_channel) => {
            let app_request = config::reset_friend_channel(
                reset_friend_channel.friend_public_key, 
                reset_friend_channel.reset_token);
            let app_to_app_server = AppToAppServer {
                app_request_id: user_request_id,
                app_request,
            };
            app_sender.send(app_to_app_server).await.map_err(|_| CompactServerError::AppSenderError)?;
        },
        // =======================[Buyer]========================================
        UserRequest::RequestPayInvoice(_request_pay_invoice) => unimplemented!(),
        UserRequest::ConfirmPayInvoice(_confirm_pay_invoice) => unimplemented!(),
        UserRequest::CancelPayInvoice(_invoice_id) => unimplemented!(),
        // =======================[Seller]=======================================
        UserRequest::AddInvoice(_add_invoice) => unimplemented!(),
        UserRequest::CancelInvoice(_invoice_id) => unimplemented!(),
        UserRequest::RequestCommitInvoice(_commit) => unimplemented!(),
    }
    unimplemented!();
}

#[allow(unused)]
async fn handle_node<US>(app_server_to_app: AppServerToApp, server_state: &mut CompactServerState, user_sender: &mut US) 
    -> Result<(), CompactServerError>
where   
    US: Sink<ToUser> + Unpin
{
    match app_server_to_app {
        AppServerToApp::TransactionResult(_transaction_result) => unimplemented!(),
        AppServerToApp::ResponseClosePayment(_response_close_payment) => unimplemented!(),
        AppServerToApp::Report(_node_report) => return Err(CompactServerError::ProtocolError),
        AppServerToApp::ReportMutations(report_mutations) => {
            // Save the original `node_report`:
            let orig_node_report = server_state.node_report.clone();

            // Apply mutations to `node_report`:
            for mutation in &report_mutations.mutations {
                server_state.node_report.mutate(mutation).map_err(|_| CompactServerError::ReportMutationError)?;
            }

            // If `node_report` has changed, send it to the user:
            if server_state.node_report != orig_node_report {
                user_sender.send(ToUser::Report(server_state.node_report.clone().into())).await.map_err(|_| CompactServerError::UserSenderError)?;
            }

            // Possibly send acknowledgement for a completed command:
            if let Some(app_request_id) = report_mutations.opt_app_request_id {
                user_sender.send(ToUser::Ack(app_request_id)).await.map_err(|_| CompactServerError::UserSenderError)?;
            }
        },
        AppServerToApp::ResponseRoutes(_client_response_routes) => unimplemented!(),
    }
    unimplemented!();
}

#[allow(unused)]
/// The compact server is mediating between the user and the node.
async fn inner_server(app_conn_tuple: AppConnTuple, 
    conn_pair_compact: ConnPairCompact, 
    mut opt_event_sender: Option<mpsc::Sender<()>>) -> Result<(), CompactServerError> {

    // Interaction with the user:
    let (mut user_sender, user_receiver) = conn_pair_compact.split();
    let (app_permissions, node_report, conn_pair_app) = app_conn_tuple;
    // Interaction with the offst node:
    let (mut app_sender, app_receiver) = conn_pair_app.split();

    let user_receiver = user_receiver.map(CompactServerEvent::User)
        .chain(stream::once(future::ready(CompactServerEvent::UserClosed)));

    let app_receiver = app_receiver.map(CompactServerEvent::Node)
        .chain(stream::once(future::ready(CompactServerEvent::NodeClosed)));

    let mut incoming_events = select_streams![
        user_receiver,
        app_receiver
    ];

    let mut server_state = CompactServerState::new(node_report);

    while let Some(event) = incoming_events.next().await {
        match event {
            CompactServerEvent::User(from_user) => handle_user(from_user, &app_permissions, &mut server_state, &mut app_sender).await?,
            CompactServerEvent::UserClosed => return Ok(()),
            CompactServerEvent::Node(app_server_to_app) => handle_node(app_server_to_app, &mut server_state, &mut user_sender).await?,
            CompactServerEvent::NodeClosed => return Ok(()),
        }
        if let Some(ref mut event_sender) = opt_event_sender {
            let _ = event_sender.send(()).await;
        }
    }
    Ok(())
}
