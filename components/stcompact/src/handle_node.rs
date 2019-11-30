use futures::{Sink, SinkExt};

use app::conn::{AppServerToApp, AppToAppServer, 
     ResponseRoutesResult, ClientResponseRoutes, RequestResult, ResponseClosePayment, buyer};
use app::common::{MultiRoute, PaymentStatus};

use route::{choose_multi_route, MultiRouteChoice};

use crate::messages::{ToUser, PaymentFees, PaymentFeesResponse, PaymentDone, PaymentCommit};
use crate::persist::{OpenPaymentStatus, OpenPaymentStatusFoundRoute};
use crate::types::{CompactServerState, CompactServerError, GenId};

/// Calculate fees if we send credits through the given MultiRoute with the MultiRouteChoice
/// strategy
fn calc_multi_route_fees(multi_route: &MultiRoute, multi_route_choice: &MultiRouteChoice) -> Option<u128> {
    let mut total_fees = 0u128;
    for (route_index, dest_payment) in multi_route_choice {
        let fee = multi_route.routes[*route_index]
            .rate
            .calc_fee(*dest_payment)?;
        total_fees = total_fees.checked_add(fee)?;
    }
    Some(total_fees)
}

fn obtain_multi_route(client_response_routes: &ClientResponseRoutes, dest_payment: u128) -> Option<(MultiRoute, MultiRouteChoice, u128)> {
    let multi_routes = match &client_response_routes.result {
        ResponseRoutesResult::Success(multi_routes) => multi_routes,
        ResponseRoutesResult::Failure => return None,
    };

    let (route_index, multi_route_choice) = 
        choose_multi_route(&multi_routes, dest_payment)?;
    let multi_route = &multi_routes[route_index];

    // Make sure that fees can be calculated correctly:
    let fees = calc_multi_route_fees(multi_route, &multi_route_choice)?;

    Some((multi_route.clone(), multi_route_choice, fees))

}

async fn ack_close_payment<GI, AS>(
    response_close_payment: &ResponseClosePayment, 
    gen_id: &mut GI,
    app_sender: &mut AS) -> Result<(), CompactServerError>
where 
    GI: GenId,
    AS: Sink<AppToAppServer> + Unpin,
{
    // Ack the payment closing if possible:
    let opt_ack_uid = match &response_close_payment.status {
        PaymentStatus::PaymentNotFound => {
            warn!("ack_close_payment: PaymentNotFound!");
            None
        },
        PaymentStatus::Success(success) => Some(success.ack_uid.clone()),
        PaymentStatus::Canceled(ack_uid) => Some(ack_uid.clone()),
    };
    if let Some(ack_uid) = opt_ack_uid {
        let app_request = buyer::ack_close_payment(response_close_payment.payment_id.clone(), ack_uid);
        let app_to_app_server = AppToAppServer {
            app_request_id: gen_id.gen_uid(),
            app_request,
        };
        app_sender.send(app_to_app_server).await.map_err(|_| CompactServerError::AppSenderError)?;
    }
    Ok(())
}

pub async fn handle_node<GI,US,AS>(app_server_to_app: AppServerToApp, 
    server_state: &mut CompactServerState, 
    gen_id: &mut GI,
    user_sender: &mut US,
    app_sender: &mut AS)
    -> Result<(), CompactServerError>
where   
    GI: GenId,
    US: Sink<ToUser> + Unpin,
    AS: Sink<AppToAppServer> + Unpin,
{
    match app_server_to_app {
        AppServerToApp::TransactionResult(transaction_result) => {
            let mut compact_state = server_state.compact_state().clone();
            let mut opt_found = None;
            for (payment_id, open_payment) in &mut compact_state.open_payments {
                match &mut open_payment.status {
                    OpenPaymentStatus::Sending(sending) => {
                        let c_sending = sending.clone();
                        if sending.open_transactions.remove(&transaction_result.request_id) {
                            opt_found = Some((payment_id.clone(), open_payment, c_sending));
                        }
                    },
                    _ => continue,
                }
            }

            let (payment_id, open_payment, sending) = if let Some(found) = opt_found {
                found
            } else {
                // We couldn't find this request. This could happen if:
                // - One of the transactions failed, so we decided to drop the list of
                // transactions.
                // - A crash happened? (Not sure about this)
                warn!("TransactionResult: Unrecognized request_id: {:?}", transaction_result.request_id);
                return Ok(());
            };

            match (transaction_result.result, sending.open_transactions.is_empty()) {
                (RequestResult::Complete(commit), _) => {
                    // Set payment status to Commit:
                    open_payment.status = OpenPaymentStatus::Commit(commit.clone(), sending.fees);
                    server_state.update_compact_state(compact_state).await?;

                    // Send commit to user:
                    let payment_commit = PaymentCommit {
                        payment_id,
                        commit: commit.into(),
                    };
                    user_sender.send(ToUser::PaymentCommit(payment_commit)).await.map_err(|_| CompactServerError::UserSenderError)?;
                },
                (RequestResult::Failure, _)
                    | (RequestResult::Success, true) => {
                    // Set payment as failed:
                    let ack_uid = gen_id.gen_uid();
                    open_payment.status = OpenPaymentStatus::Failure(ack_uid.clone());
                    server_state.update_compact_state(compact_state).await?;

                    // Inform the user about failure.
                    // Send a message about payment done:
                    let payment_done = PaymentDone::Failure(ack_uid);
                    user_sender.send(ToUser::PaymentDone(payment_done)).await.map_err(|_| CompactServerError::UserSenderError)?;
                },
                (RequestResult::Success, false) => {
                    // There are still pending transactions. We will have to wait for the next
                    // transactions to complete.
                },
            };
        },
        AppServerToApp::ResponseClosePayment(response_close_payment) => {
            let mut compact_state = server_state.compact_state().clone();
            let open_payment = if let Some(open_payment) = compact_state.open_payments.get_mut(&response_close_payment.payment_id) {
                open_payment
            } else {
                warn!("ResponseClosePayment: Unrecognized payment_id: {:?}", response_close_payment.payment_id);
                ack_close_payment(&response_close_payment, gen_id, app_sender).await?;
                return Ok(());
            };

            match open_payment.clone().status {
                OpenPaymentStatus::SearchingRoute(_)
                | OpenPaymentStatus::FoundRoute(_) => {
                    warn!("ResponseClosePayment: Node closed payment before we opened it! payment_id {:?}", response_close_payment.payment_id);
                },
                OpenPaymentStatus::Sending(_sending) => {
                    // We expect failure here. It is not likely that the payment was successful if
                    // we never got a commit to hand to the seller.
                    let ack_uid = gen_id.gen_uid();
                    open_payment.status = OpenPaymentStatus::Failure(ack_uid.clone());
                    server_state.update_compact_state(compact_state).await?;

                    // Inform the user about failure.
                    // Send a message about payment done:
                    let payment_done = PaymentDone::Failure(ack_uid);
                    user_sender.send(ToUser::PaymentDone(payment_done)).await.map_err(|_| CompactServerError::UserSenderError)?;
                },
                OpenPaymentStatus::Commit(_commit, fees) => {
                    // This is the most common state to get a `ResponseClosePayment`.
                    // We will now be able to know whether the payment succeeded (and we get a receipt),
                    // or failed.
                    match &response_close_payment.status {
                        PaymentStatus::PaymentNotFound 
                        | PaymentStatus::Canceled(_) => {
                            // Set payment to failure:
                            let ack_uid = gen_id.gen_uid();
                            open_payment.status = OpenPaymentStatus::Failure(ack_uid.clone());
                            server_state.update_compact_state(compact_state).await?;

                            // Inform the user about failure.
                            // Send a message about payment done: 
                            let payment_done = PaymentDone::Failure(ack_uid);
                            user_sender.send(ToUser::PaymentDone(payment_done)).await.map_err(|_| CompactServerError::UserSenderError)?;
                        },
                        PaymentStatus::Success(success) => {
                            // Set payment to success:
                            let ack_uid = gen_id.gen_uid();
                            open_payment.status = OpenPaymentStatus::Success(success.receipt.clone(), fees, ack_uid.clone());
                            server_state.update_compact_state(compact_state).await?;

                            // Inform the user about success.
                            // Send a message about payment done: 
                            let payment_done = PaymentDone::Success(success.receipt.clone(), fees, ack_uid);
                            user_sender.send(ToUser::PaymentDone(payment_done)).await.map_err(|_| CompactServerError::UserSenderError)?;
                        },
                    };
                },
                OpenPaymentStatus::Failure(_) => {
                    // This could happen if one of our transactions failed, an we already set the
                    // status to failed ourselves.
                },
                OpenPaymentStatus::Success(_, _, _) => {
                    warn!("ResponseClosePayment: Node sent ResponseClosePayment more than once! payment_id {:?}", response_close_payment.payment_id);
                },
            }
            // In any case, we acknowledge the `ResponseClosePayment` message, to make sure the
            // done payment is not stuck forever inside the node:
            ack_close_payment(&response_close_payment, gen_id, app_sender).await?;
        },
        AppServerToApp::ReportMutations(report_mutations) => {
            // Save the original `node_report`:
            let mut node_report = server_state.node_report().clone();

            // Apply mutations to `node_report`:
            for mutation in &report_mutations.mutations {
                node_report.mutate(mutation).map_err(|_| CompactServerError::ReportMutationError)?;
            }

            // If `node_report` has changed, send it to the user:
            if &node_report != server_state.node_report() {
                server_state.update_node_report(node_report.clone());
                user_sender.send(ToUser::Report(node_report.clone().into())).await.map_err(|_| CompactServerError::UserSenderError)?;
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

            let (multi_route, multi_route_choice, fees) = if let Some(inner) = obtain_multi_route(&client_response_routes, open_payment.dest_payment) {
                inner
            } else {
                // A suitable route was not found.
                
                // Close the payment.
                let _ = compact_state.open_payments.remove(&payment_id).unwrap();
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
            let found_route = OpenPaymentStatusFoundRoute {
                confirm_id: confirm_id.clone(),
                multi_route,
                multi_route_choice,
                fees,
            };
            open_payment.status = OpenPaymentStatus::FoundRoute(found_route);
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
