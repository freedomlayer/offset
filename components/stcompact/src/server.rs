use futures::{future, stream, StreamExt, channel::mpsc, Sink, SinkExt};

use common::select_streams::select_streams;
use common::conn::{ConnPair, BoxStream};

use database::{DatabaseClient};

#[allow(unused)]
use app::conn::{AppConnTuple, AppServerToApp, AppToAppServer, AppPermissions, buyer, config, routes, seller};
use app::report::NodeReport;

use crate::types::{FromUser, ToUser, UserRequest};
use crate::persist::{CompactState, OpenInvoice};

type ConnPairCompact = ConnPair<ToUser, FromUser>;

#[derive(Debug)]
enum CompactServerEvent {
    User(FromUser),
    UserClosed,
    Node(AppServerToApp),
    NodeClosed,
}

pub enum CompactServerError {
    AppSenderError,
    UserSenderError,
    ReportMutationError,
    DatabaseMutateError,
}

#[allow(unused)]
struct CompactServerState {
    node_report: NodeReport,
    compact_state: CompactState,
    database_client: DatabaseClient<CompactState>,
}

#[allow(unused)]
impl CompactServerState {
    pub fn new(node_report: NodeReport, compact_state: CompactState, database_client: DatabaseClient<CompactState>) -> Self {
        Self {
            node_report,
            compact_state,
            database_client,
        }
    }

    /// Get current `node_update`
    pub fn node_report(&self) -> &NodeReport {
        &self.node_report
    }

    pub fn update_node_report(&mut self, node_report: NodeReport) {
        self.node_report = node_report;
    }

    /// Get current `compact_state`
    pub fn compact_state(&self) -> &CompactState {
        &self.compact_state
    }

    /// Persistent (and atomic) update to `compact_state`
    pub async fn update_compact_state(&mut self, compact_state: CompactState) -> Result<(), CompactServerError> {
        self.compact_state = compact_state.clone();
        self.database_client.mutate(vec![compact_state])
            .await
            .map_err(|_| CompactServerError::DatabaseMutateError)?;
        Ok(())
    }
}

#[allow(unused)]
// TODO: Should we check permissions here in the future?
// Permissions are already checked on the node side (offst-app-server). I don't want to have code duplication here for
// permissions.
async fn handle_user<AS, US>(from_user: FromUser, _app_permissions: &AppPermissions, 
    server_state: &mut CompactServerState, user_sender: &mut US, app_sender: &mut AS) 
    -> Result<(), CompactServerError>
where   
    US: Sink<ToUser> + Unpin,
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
        UserRequest::AddInvoice(add_invoice) => {
            let mut compact_state = server_state.compact_state().clone();
            if compact_state.open_invoices.contains_key(&add_invoice.invoice_id) {
                // Invoice already Open:
                warn!("AddInvoice: Invoice {:?} is already open!", add_invoice.invoice_id);
                return user_sender.send(ToUser::Ack(user_request_id)).await.map_err(|_| CompactServerError::UserSenderError);
            }

            let open_invoice = OpenInvoice {
                currency: add_invoice.currency.clone(),
                total_dest_payment: add_invoice.total_dest_payment.clone(),
                description: add_invoice.description,
            };
            compact_state.open_invoices.insert(add_invoice.invoice_id.clone(), open_invoice);
            // Order:
            // - Update local database
            // - Send a message to add invoice
            //
            // Note that we first update our local persistent database, and only then send a
            // message to the node. The order here is crucial: If a crash happens, we will the open
            // invoice in our persistent database, and we will be able to resend it.
            server_state.update_compact_state(compact_state).await?;

            let app_request = seller::add_invoice(
                add_invoice.invoice_id, 
                add_invoice.currency,
                add_invoice.total_dest_payment);

            let app_to_app_server = AppToAppServer {
                app_request_id: user_request_id,
                app_request,
            };
            app_sender.send(app_to_app_server).await.map_err(|_| CompactServerError::AppSenderError)?;
        },
        UserRequest::CancelInvoice(invoice_id) => {
            // If invoice is not listed as open, we return an ack and do nothing:
            let mut compact_state = server_state.compact_state().clone();
            if !compact_state.open_invoices.contains_key(&invoice_id) {
                // Invoice is not open:
                warn!("CancelInvoice: Invoice {:?} is not open!", invoice_id);
                return user_sender.send(ToUser::Ack(user_request_id)).await.map_err(|_| CompactServerError::UserSenderError);
            }

            // Order:
            // - Send cancellation message
            // - Update local database
            //
            // Note that here we send a cancellation message, and only then update our local
            // persistent database (Reversed order with respect to AddInvoice).
            // If a crash happens, our local database will still indicate that there is still an
            // open invoice.

            // Send cancellation message:
            let app_request = seller::cancel_invoice(invoice_id.clone());

            let app_to_app_server = AppToAppServer {
                app_request_id: user_request_id,
                app_request,
            };
            app_sender.send(app_to_app_server).await.map_err(|_| CompactServerError::AppSenderError)?;

            // Update local database:
            compact_state.open_invoices.remove(&invoice_id);
            // Note that we first update our local persistent database, and only then send a
            // message to the node. The order here is crucial: If a crash happens, we will the open
            // invoice in our persistent database, and we will be able to resend it.
            server_state.update_compact_state(compact_state).await?;
        },
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

/// The compact server is mediating between the user and the node.
async fn inner_server(app_conn_tuple: AppConnTuple, 
    conn_pair_compact: ConnPairCompact, 
    compact_state: CompactState,
    database_client: DatabaseClient<CompactState>,
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

    let mut server_state = CompactServerState::new(node_report, compact_state, database_client);

    while let Some(event) = incoming_events.next().await {
        match event {
            CompactServerEvent::User(from_user) => handle_user(from_user, &app_permissions, &mut server_state, &mut user_sender, &mut app_sender).await?,
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

#[allow(unused)]
pub async fn server(app_conn_tuple: AppConnTuple, 
    conn_pair_compact: ConnPairCompact,
    compact_state: CompactState,
    database_client: DatabaseClient<CompactState>) -> Result<(), CompactServerError> {

    inner_server(app_conn_tuple, conn_pair_compact, compact_state, database_client, None).await
}
