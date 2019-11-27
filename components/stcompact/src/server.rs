use futures::{future, stream, StreamExt, channel::mpsc, Sink, SinkExt};

use common::select_streams::select_streams;
use common::conn::{ConnPair, BoxStream};

use app::conn::{AppConnTuple, AppServerToApp, AppToAppServer, AppPermissions};
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

enum CompactServerError {}

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
        UserRequest::AddRelay(_add_relay) => unimplemented!(),
        UserRequest::RemoveRelay(_remove_relay) => unimplemented!(),
        UserRequest::AddIndexServer(_named_index_server_address) => unimplemented!(),
        UserRequest::RemoveIndexServer(_public_key) => unimplemented!(),
        UserRequest::AddFriend(_add_friend) => unimplemented!(),
        UserRequest::SetFriendRelays(_set_friend_relays) => unimplemented!(),
        UserRequest::SetFriendName(_set_friend_name) => unimplemented!(),
        UserRequest::RemoveFriend(_friend_public_key) => unimplemented!(),
        UserRequest::EnableFriend(_friend_public_key) => unimplemented!(),
        UserRequest::DisableFriend(_friend_public_key) => unimplemented!(),
        UserRequest::OpenFriendCurrency(_open_friend_currency) => unimplemented!(),
        UserRequest::CloseFriendCurrency(_close_friend_currency) => unimplemented!(),
        UserRequest::SetFriendCurrencyMaxDebt(_set_friend_currency_max_debt) => unimplemented!(),
        UserRequest::SetFriendCurrencyRate(_set_friend_currency_rate) => unimplemented!(),
        UserRequest::RemoveFriendCurrency(_remove_friend_currency) => unimplemented!(),
        UserRequest::ResetFriendChannel(_reset_friend_channel) => unimplemented!(),
        UserRequest::RequestPayInvoice(_request_pay_invoice) => unimplemented!(),
        UserRequest::ConfirmPayInvoice(_confirm_pay_invoice) => unimplemented!(),
        UserRequest::CancelPayInvoice(_invoice_id) => unimplemented!(),
        UserRequest::AddInvoice(_add_invoice) => unimplemented!(),
        UserRequest::CancelInvoice(_invoice_id) => unimplemented!(),
        UserRequest::CommitInvoice(_commit) => unimplemented!(),
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
        AppServerToApp::Report(_node_report) => unimplemented!(),
        AppServerToApp::ReportMutations(_report_mutations) => unimplemented!(),
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
