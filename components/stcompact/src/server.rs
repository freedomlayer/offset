use futures::{future, stream, StreamExt, channel::mpsc, Sink, SinkExt};

use common::select_streams::select_streams;
use common::conn::{ConnPair, BoxStream};

use database::{DatabaseClient};

#[allow(unused)]
use app::conn::{AppConnTuple, AppServerToApp, AppToAppServer, AppPermissions, 
    buyer, config, routes, seller, ResponseRoutesResult, ClientResponseRoutes};
use app::common::{Uid, PaymentId, MultiRoute};
use app::verify::verify_commit;

use route::choose_multi_route;

use crate::types::{FromUser, ToUser, UserRequest, ResponseCommitInvoice, PaymentFees, PaymentFeesResponse};
use crate::persist::{CompactState, OpenInvoice, OpenPayment, OpenPaymentStatus};

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
    node_report: app::report::NodeReport,
    compact_state: CompactState,
    database_client: DatabaseClient<CompactState>,
}

#[allow(unused)]
impl CompactServerState {
    pub fn new(node_report: app::report::NodeReport, compact_state: CompactState, database_client: DatabaseClient<CompactState>) -> Self {
        Self {
            node_report,
            compact_state,
            database_client,
        }
    }

    /// Get current `node_update`
    pub fn node_report(&self) -> &app::report::NodeReport {
        &self.node_report
    }

    pub fn update_node_report(&mut self, node_report: app::report::NodeReport) {
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

pub trait GenId {
    /// Generate a Uid
    fn gen_uid(&mut self) -> Uid;

    /// Generate a PaymentId
    fn gen_payment_id(&mut self) -> PaymentId;
}

fn obtain_multi_route(client_response_routes: &ClientResponseRoutes, dest_payment: u128) -> Option<(MultiRoute, u128)> {
    let multi_routes = match &client_response_routes.result {
        ResponseRoutesResult::Success(multi_routes) => multi_routes,
        ResponseRoutesResult::Failure => return None,
    };

    let (route_index, multi_route_choice) = 
        choose_multi_route(&multi_routes, dest_payment)?;
    let multi_route = &multi_routes[route_index];

    // Calcualte total fees
    let mut total_fees = 0u128;
    for (route_index, dest_payment) in &multi_route_choice {
        let fee = multi_route.routes[*route_index]
            .rate
            .calc_fee(*dest_payment)?;
        total_fees = total_fees.checked_add(fee)?;
    }

    Some((multi_route.clone(), total_fees))

}

#[allow(unused)]
// TODO: Should we check permissions here in the future?
// Permissions are already checked on the node side (offst-app-server). I don't want to have code duplication here for
// permissions.
async fn handle_user<AS, US, GI>(
    from_user: FromUser, 
    _app_permissions: &AppPermissions, 
    server_state: &mut CompactServerState, 
    gen_id: &mut GI,
    user_sender: &mut US, 
    app_sender: &mut AS) 
    -> Result<(), CompactServerError>
where   
    US: Sink<ToUser> + Unpin,
    AS: Sink<AppToAppServer> + Unpin,
    GI: GenId,
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
        UserRequest::InitPayment(init_payment) => {

            let mut compact_state = server_state.compact_state().clone();

            if let Some(open_payment) = compact_state.open_payments.get(&init_payment.payment_id) {
                // We might need to resend to the user the current state of the payment.
                match &open_payment.status {
                    OpenPaymentStatus::SearchingRoute(_) => return Ok(()),
                    OpenPaymentStatus::FoundRoute(confirm_id, _multi_route, fees) => {
                        // We have already sent a ResponsePayInvoice, but the user might have not
                        // received it, or forgotten that it did due to a crash.
                        
                        // Send Ack:
                        user_sender.send(ToUser::Ack(user_request_id)).await.map_err(|_| CompactServerError::UserSenderError)?;

                        // Resend PaymentFees message to the user:
                        let payment_fees = PaymentFees {
                            payment_id: init_payment.payment_id.clone(),
                            response: PaymentFeesResponse::Fees(*fees, confirm_id.clone()),
                        };
                        return user_sender.send(ToUser::PaymentFees(payment_fees)).await.map_err(|_| CompactServerError::UserSenderError);
                    },
                    OpenPaymentStatus::Sending(_) | OpenPaymentStatus::Commit(_,_) => {
                        // Payment already in progress, and the user should know it.
                        warn!("RequestPayInvoice: Paymenet for invoice {:?} is already open!", init_payment.invoice_id);
                        return user_sender.send(ToUser::Ack(user_request_id)).await.map_err(|_| CompactServerError::UserSenderError);
                    },
                }

            }

            // Generate a request_routes_id:
            let request_routes_id = gen_id.gen_uid();

            // Request routes:
            let opt_exclude = None;
            let app_request = routes::request_routes(
                request_routes_id.clone(),
                init_payment.currency.clone(),
                init_payment.dest_payment.clone(),
                server_state.node_report().funder_report.local_public_key.clone(),
                init_payment.dest_public_key.clone(),
                opt_exclude);

            let app_to_app_server = AppToAppServer {
                app_request_id: user_request_id,
                app_request,
            };
            app_sender.send(app_to_app_server).await.map_err(|_| CompactServerError::AppSenderError)?;

            let open_payment = OpenPayment {
                invoice_id: init_payment.invoice_id,
                currency: init_payment.currency,
                dest_public_key: init_payment.dest_public_key,
                dest_payment: init_payment.dest_payment,
                description: init_payment.description,
                status: OpenPaymentStatus::SearchingRoute(request_routes_id),
            };
            compact_state.open_payments.insert(init_payment.payment_id.clone(), open_payment);

            server_state.update_compact_state(compact_state).await?;
        },
        UserRequest::ConfirmPaymentFees(_confirm_payment_fees) => {
            /*
            let mut compact_state = server_state.compact_state().clone();
            if compact_state.open_payments.get(&confirm_payment_fees.invoice_id) {
            }
            */
            unimplemented!();
        },
        UserRequest::CancelPayment(_invoice_id) => unimplemented!(),
        UserRequest::AckReceipt(()) => unimplemented!(),
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
        UserRequest::RequestCommitInvoice(commit) => {
            // Make sure that the corresponding invoice is open:
            let mut compact_state = server_state.compact_state().clone();
            let open_invoice = if let Some(open_invoice) = compact_state.open_invoices.get(&commit.invoice_id) {
                open_invoice
            } else {
                // Invoice is not open:
                warn!("RequestCommitInvoice: Invoice {:?} is not open!", commit.invoice_id);
                return user_sender.send(ToUser::Ack(user_request_id)).await.map_err(|_| CompactServerError::UserSenderError);
            };

            let node_commit = commit.clone().into();

            // Verify commitment
            if !verify_commit(&node_commit, &server_state.node_report().funder_report.local_public_key) {
                warn!("RequestCommitInvoice: Invoice: {:?}: Invalid commit", commit.invoice_id);
                user_sender.send(ToUser::Ack(user_request_id)).await.map_err(|_| CompactServerError::UserSenderError);
                return user_sender.send(ToUser::ResponseCommitInvoice(ResponseCommitInvoice::Failure)).await.map_err(|_| CompactServerError::UserSenderError);
            }

            // Send commitment to node:
            let app_request = seller::commit_invoice(node_commit);

            let app_to_app_server = AppToAppServer {
                app_request_id: user_request_id,
                app_request,
            };
            app_sender.send(app_to_app_server).await.map_err(|_| CompactServerError::AppSenderError)?;

            // Update local database:
            compact_state.open_invoices.remove(&commit.invoice_id);
            // Note that we first update our local persistent database, and only then send a
            // message to the node. The order here is crucial: If a crash happens, we will the open
            // invoice in our persistent database, and we will be able to resend it.
            server_state.update_compact_state(compact_state).await?;

            // Send indication to user that the commitment is successful:
            return user_sender.send(ToUser::ResponseCommitInvoice(ResponseCommitInvoice::Success)).await.map_err(|_| CompactServerError::UserSenderError);
        },
    }
    Ok(())
}

#[allow(unused)]
async fn handle_node<GI,US>(app_server_to_app: AppServerToApp, 
    server_state: &mut CompactServerState, 
    gen_id: &mut GI,
    user_sender: &mut US) 
    -> Result<(), CompactServerError>
where   
    US: Sink<ToUser> + Unpin,
    GI: GenId,
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
        AppServerToApp::ResponseRoutes(mut client_response_routes) => {
            // Search for the corresponding OpenPayment:
            let mut compact_state = server_state.compact_state().clone();
            let mut opt_invoice_id_open_payment = None;
            for (payment_id, open_payment) in &mut compact_state.open_payments {
                if let OpenPaymentStatus::SearchingRoute(request_routes_id) = &mut open_payment.status {
                    if request_routes_id == &mut client_response_routes.request_id {
                        opt_invoice_id_open_payment = Some((payment_id.clone(), open_payment));
                    }
                }
            }

            let (payment_id, open_payment) = if let Some(invoice_id_open_payment) = opt_invoice_id_open_payment {
                invoice_id_open_payment
            } else {
                // We don't remember this request
                warn!("ResponseRoutes: Unrecognized request_routes_id: {:?}", client_response_routes.request_id);
                return Ok(());
            };

            let (multi_route, fees) = if let Some(multi_route_fees) = obtain_multi_route(&client_response_routes, open_payment.dest_payment) {
                multi_route_fees
            } else {
                // A suitable route was not found.
                
                // Close the payment.
                let open_payment = compact_state.open_payments.remove(&payment_id).unwrap();
                server_state.update_compact_state(compact_state).await?;

                // Notify user that the payment has failed:
                let payment_fees = PaymentFees {
                    payment_id,
                    response: PaymentFeesResponse::Unreachable,
                };
                return user_sender.send(ToUser::PaymentFees(payment_fees)).await.map_err(|_| CompactServerError::UserSenderError);
            };

            // Update compact state (keep the best multiroute):
            let confirm_id = gen_id.gen_uid();
            open_payment.status = OpenPaymentStatus::FoundRoute(confirm_id.clone(), multi_route, fees);
            server_state.update_compact_state(compact_state).await?;

            // Notify user that a route was found (Send required fees):
            let payment_fees = PaymentFees {
                payment_id,
                response: PaymentFeesResponse::Fees(fees, confirm_id),
            };
            return user_sender.send(ToUser::PaymentFees(payment_fees)).await.map_err(|_| CompactServerError::UserSenderError);
        }
    }
    Ok(())
}

/// The compact server is mediating between the user and the node.
async fn inner_server<GI>(app_conn_tuple: AppConnTuple, 
    conn_pair_compact: ConnPairCompact, 
    compact_state: CompactState,
    database_client: DatabaseClient<CompactState>,
    mut gen_id: GI,
    mut opt_event_sender: Option<mpsc::Sender<()>>) -> Result<(), CompactServerError> 
where
    GI: GenId,
{

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
            CompactServerEvent::User(from_user) => handle_user(from_user, &app_permissions, &mut server_state, &mut gen_id, &mut user_sender, &mut app_sender).await?,
            CompactServerEvent::UserClosed => return Ok(()),
            CompactServerEvent::Node(app_server_to_app) => handle_node(app_server_to_app, &mut server_state, &mut gen_id, &mut user_sender).await?,
            CompactServerEvent::NodeClosed => return Ok(()),
        }
        if let Some(ref mut event_sender) = opt_event_sender {
            let _ = event_sender.send(()).await;
        }
    }
    Ok(())
}

#[allow(unused)]
pub async fn server<GI>(app_conn_tuple: AppConnTuple, 
    conn_pair_compact: ConnPairCompact,
    compact_state: CompactState,
    database_client: DatabaseClient<CompactState>,
    gen_id: GI) -> Result<(), CompactServerError> 
where   
    GI: GenId,
{
    inner_server(app_conn_tuple, conn_pair_compact, compact_state, database_client, gen_id, None).await
}
