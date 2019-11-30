use futures::{Sink, SinkExt};

use app::conn::{AppToAppServer, AppPermissions, 
    buyer, config, routes, seller};
use app::common::Uid;
use app::verify::verify_commit;

use crate::messages::{FromUser, ToUser, UserRequest, ResponseCommitInvoice, 
    PaymentFees, PaymentFeesResponse, PaymentDone};
use crate::persist::{OpenInvoice, OpenPayment, OpenPaymentStatus, OpenPaymentStatusSending};
use crate::types::{CompactServerState, CompactServerError, GenId};



// TODO: Should we check permissions here in the future?
// Permissions are already checked on the node side (offst-app-server). I don't want to have code duplication here for
// permissions.
pub async fn handle_user<GI,US,AS>(
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
                    OpenPaymentStatus::FoundRoute(found_route) => {
                        // We have already sent a ResponsePayInvoice, but the user might have not
                        // received it, or forgotten that it did due to a crash.
                        
                        // Send Ack:
                        user_sender.send(ToUser::Ack(user_request_id)).await.map_err(|_| CompactServerError::UserSenderError)?;

                        // Resend PaymentFees message to the user:
                        let payment_fees = PaymentFees {
                            payment_id: init_payment.payment_id.clone(),
                            response: PaymentFeesResponse::Fees(found_route.fees, found_route.confirm_id.clone()),
                        };
                        return user_sender.send(ToUser::PaymentFees(payment_fees)).await.map_err(|_| CompactServerError::UserSenderError);
                    },
                    OpenPaymentStatus::Sending(_) 
                        | OpenPaymentStatus::Commit(_,_) 
                        | OpenPaymentStatus::Success(_,_,_) 
                        | OpenPaymentStatus::Failure(_) => {
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
        UserRequest::ConfirmPaymentFees(confirm_payment_fees) => {
            let mut compact_state = server_state.compact_state().clone();
            let opt_inner = if let Some(open_payment) = compact_state.open_payments.get(&confirm_payment_fees.payment_id) {
                match &open_payment.status {
                    OpenPaymentStatus::SearchingRoute(_) => None,
                    OpenPaymentStatus::FoundRoute(found_route) => {
                        if confirm_payment_fees.confirm_id == found_route.confirm_id {
                            // TODO: cloning here might not be the most efficient approach?
                            Some((found_route.multi_route.clone(), found_route.multi_route_choice.clone(), found_route.fees))
                        } else {
                            // confirm_id doesn't match:
                            None
                        }
                    },
                    OpenPaymentStatus::Sending(_)
                    | OpenPaymentStatus::Commit(_,_)
                    | OpenPaymentStatus::Success(_,_,_)
                    | OpenPaymentStatus::Failure(_) => None
                }
            } else {
                // No such payment in progress.
                None
            };

            let (multi_route, multi_route_choice, fees) = if let Some(inner) = opt_inner {
                inner
            } else {
                // Send acknowledgement to user:
                return user_sender.send(ToUser::Ack(user_request_id)).await.map_err(|_| CompactServerError::UserSenderError);
            };

            // Order:
            // - Update local database
            // - Send requests along routes

            // Update compact_state:
            let open_transactions: Vec<Uid> = multi_route_choice
                .iter()
                .map(|_| gen_id.gen_uid())
                .collect();

            let sending = OpenPaymentStatusSending {
                fees,
                open_transactions: open_transactions.clone().into_iter().collect(),
            };

            let open_payment = compact_state.open_payments
                    .get_mut(&confirm_payment_fees.payment_id)
                    .unwrap();

            open_payment.status = OpenPaymentStatus::Sending(sending);
            let c_open_payment = open_payment.clone();

            server_state.update_compact_state(compact_state).await?;

            // Create a new payment:
            let app_request = buyer::create_payment(
                confirm_payment_fees.payment_id.clone(),
                c_open_payment.invoice_id,
                c_open_payment.currency,
                c_open_payment.dest_payment,
                c_open_payment.dest_public_key);

            let app_to_app_server = AppToAppServer {
                // This is an `app_request_id` we don't need to track:
                app_request_id: gen_id.gen_uid(),
                app_request,
            };
            app_sender.send(app_to_app_server).await.map_err(|_| CompactServerError::AppSenderError)?;

            // Initiate requests along all routes in the multi route, where credits
            // are allocated according to the strategy in `multi_route_choice`:
            for ((route_index, dest_payment), request_id) in multi_route_choice.iter().cloned().zip(open_transactions) {
            
                let route = &multi_route.routes[route_index];

                let app_request = buyer::create_transaction(
                    confirm_payment_fees.payment_id.clone(),
                    request_id,
                    route.route.clone(),
                    dest_payment,
                    route.rate.calc_fee(dest_payment).unwrap(),
                );

                let app_to_app_server = AppToAppServer {
                    // We don't really care about app_request_id here, as we can wait on `request_id`
                    // instead.
                    app_request_id: gen_id.gen_uid(),
                    app_request,
                };
                app_sender.send(app_to_app_server).await.map_err(|_| CompactServerError::AppSenderError)?;
            }

            // Send RequestClosePayment, as we are not going to send any more transactions:
            let app_request = buyer::request_close_payment(confirm_payment_fees.payment_id.clone());
            let app_to_app_server = AppToAppServer {
                // We assign `user_request_id` here. This will provide the user with an ack for this
                // request.
                app_request_id: user_request_id,
                app_request,
            };
            app_sender.send(app_to_app_server).await.map_err(|_| CompactServerError::AppSenderError)?;
        },
        UserRequest::CancelPayment(payment_id) => {
            let mut compact_state = server_state.compact_state().clone();
            let open_payment = if let Some(open_payment) = compact_state.open_payments.get_mut(&payment_id) {
                open_payment
            } else {
                warn!("CancelPayment: payment {:?} is not open!", payment_id);
                return user_sender.send(ToUser::Ack(user_request_id)).await.map_err(|_| CompactServerError::UserSenderError);
            };

            match open_payment.clone().status {
                OpenPaymentStatus::SearchingRoute(_)
                | OpenPaymentStatus::FoundRoute(_)
                | OpenPaymentStatus::Sending(_) => {
                    // Set failure status:
                    let ack_uid = gen_id.gen_uid();
                    open_payment.status = OpenPaymentStatus::Failure(ack_uid.clone());
                    server_state.update_compact_state(compact_state).await?;

                    // Send ack:
                    user_sender.send(ToUser::Ack(user_request_id)).await.map_err(|_| CompactServerError::UserSenderError)?;

                    // Inform the user about failure.
                    // Send a message about payment done:
                    let payment_done = PaymentDone::Failure(ack_uid);
                    user_sender.send(ToUser::PaymentDone(payment_done)).await.map_err(|_| CompactServerError::UserSenderError)?;
                },
                OpenPaymentStatus::Commit(_commit, _fees) => {
                    // We do not know if the user has already provided the commit message to the
                    // seller. If so, it might not be possible to cancel the payment.
                    //
                    // We ack the user that we received this request, but we have nothing to do
                    // about it but wait.
                    user_sender.send(ToUser::Ack(user_request_id)).await.map_err(|_| CompactServerError::UserSenderError)?;
                },
                OpenPaymentStatus::Success(_,_,_) 
                | OpenPaymentStatus::Failure(_) => {
                    warn!("CancelPayment: payment {:?} is already done!", payment_id);
                    user_sender.send(ToUser::Ack(user_request_id)).await.map_err(|_| CompactServerError::UserSenderError)?;
                },
            }

        },
        UserRequest::AckPaymentDone(payment_id, ack_uid) => {
            let mut compact_state = server_state.compact_state().clone();
            let open_payment = if let Some(open_payment) = compact_state.open_payments.get(&payment_id) {
                open_payment
            } else {
                warn!("AckPaymentDone: payment {:?} does not exist!", payment_id);
                return user_sender.send(ToUser::Ack(user_request_id)).await.map_err(|_| CompactServerError::UserSenderError);
            };

            match &open_payment.status {
                OpenPaymentStatus::SearchingRoute(_)
                | OpenPaymentStatus::FoundRoute(_)
                | OpenPaymentStatus::Sending(_)
                | OpenPaymentStatus::Commit(_, _) => {
                    warn!("AckPaymentDone: payment {:?} is not done!", payment_id);
                    return user_sender.send(ToUser::Ack(user_request_id)).await.map_err(|_| CompactServerError::UserSenderError);
                },
                OpenPaymentStatus::Success(_, _, stored_ack_uid)
                | OpenPaymentStatus::Failure(stored_ack_uid) => {
                    if stored_ack_uid == &ack_uid {
                        let _ = compact_state.open_payments.remove(&payment_id).unwrap();
                        server_state.update_compact_state(compact_state).await?;
                    }
                    return user_sender.send(ToUser::Ack(user_request_id)).await.map_err(|_| CompactServerError::UserSenderError);
                },
            }
        },
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
            if !compact_state.open_invoices.contains_key(&commit.invoice_id) {
                // Invoice is not open:
                warn!("RequestCommitInvoice: Invoice {:?} is not open!", commit.invoice_id);
                return user_sender.send(ToUser::Ack(user_request_id)).await.map_err(|_| CompactServerError::UserSenderError);
            };

            let node_commit = commit.clone().into();

            // Verify commitment
            if !verify_commit(&node_commit, &server_state.node_report().funder_report.local_public_key) {
                warn!("RequestCommitInvoice: Invoice: {:?}: Invalid commit", commit.invoice_id);
                user_sender.send(ToUser::Ack(user_request_id)).await.map_err(|_| CompactServerError::UserSenderError)?;
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
