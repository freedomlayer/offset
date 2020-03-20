use futures::{Sink, SinkExt};

use app::common::{MultiRoute, PaymentStatus};
use app::conn::{
    buyer, AppServerToApp, AppToAppServer, ClientResponseRoutes, RequestResult,
    ResponseClosePayment, ResponseRoutesResult,
};

use route::{choose_multi_route, MultiRouteChoice};

use crate::compact_node::convert::create_compact_report;
use crate::compact_node::messages::{
    CompactToUser, CompactToUserAck, PaymentCommit, PaymentDone, PaymentDoneStatus, PaymentFees,
    PaymentFeesResponse,
};
use crate::compact_node::persist::{OpenPaymentStatus, OpenPaymentStatusFoundRoute};
use crate::compact_node::types::{CompactNodeError, CompactServerState};
use crate::gen::GenUid;

use crate::compact_node::utils::update_send_compact_state;

/// Calculate fees if we send credits through the given MultiRoute with the MultiRouteChoice
/// strategy
fn calc_multi_route_fees(
    multi_route: &MultiRoute,
    multi_route_choice: &[(usize, u128)],
) -> Option<u128> {
    let mut total_fees = 0u128;
    for (route_index, dest_payment) in multi_route_choice {
        let fee = multi_route.routes[*route_index]
            .rate
            .calc_fee(*dest_payment)?;
        total_fees = total_fees.checked_add(fee)?;
    }
    Some(total_fees)
}

fn obtain_multi_route(
    client_response_routes: &ClientResponseRoutes,
    dest_payment: u128,
) -> Option<(MultiRoute, MultiRouteChoice, u128)> {
    let multi_routes = match &client_response_routes.result {
        ResponseRoutesResult::Success(multi_routes) => multi_routes,
        ResponseRoutesResult::Failure => return None,
    };

    let (route_index, multi_route_choice) = choose_multi_route(&multi_routes, dest_payment)?;
    let multi_route = &multi_routes[route_index];

    // Make sure that fees can be calculated correctly:
    let fees = calc_multi_route_fees(multi_route, &multi_route_choice)?;

    Some((multi_route.clone(), multi_route_choice, fees))
}

async fn ack_close_payment<CG, AS>(
    response_close_payment: &ResponseClosePayment,
    compact_gen: &mut CG,
    app_sender: &mut AS,
) -> Result<(), CompactNodeError>
where
    CG: GenUid,
    AS: Sink<AppToAppServer> + Unpin,
{
    // Ack the payment closing if possible:
    let opt_ack_uid = match &response_close_payment.status {
        PaymentStatus::PaymentNotFound => {
            warn!("ack_close_payment: PaymentNotFound!");
            None
        }
        PaymentStatus::Success(success) => Some(success.ack_uid.clone()),
        PaymentStatus::Canceled(ack_uid) => Some(ack_uid.clone()),
    };
    if let Some(ack_uid) = opt_ack_uid {
        let app_request =
            buyer::ack_close_payment(response_close_payment.payment_id.clone(), ack_uid);
        let app_to_app_server = AppToAppServer {
            app_request_id: compact_gen.gen_uid(),
            app_request,
        };
        app_sender
            .send(app_to_app_server)
            .await
            .map_err(|_| CompactNodeError::AppSenderError)?;
    }
    Ok(())
}

#[allow(clippy::cognitive_complexity)]
pub async fn handle_node<CG, US, AS>(
    app_server_to_app: AppServerToApp,
    server_state: &mut CompactServerState,
    compact_gen: &mut CG,
    user_sender: &mut US,
    app_sender: &mut AS,
) -> Result<(), CompactNodeError>
where
    CG: GenUid,
    US: Sink<CompactToUserAck> + Unpin,
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
                        if sending
                            .open_transactions
                            .remove(&transaction_result.request_id)
                        {
                            opt_found = Some((payment_id.clone(), open_payment, c_sending));
                        }
                    }
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
                warn!(
                    "TransactionResult: Unrecognized request_id: {:?}",
                    transaction_result.request_id
                );
                return Ok(());
            };

            match (
                transaction_result.result,
                sending.open_transactions.is_empty(),
            ) {
                (RequestResult::Complete(commit), _) => {
                    // Set payment status to Commit:
                    open_payment.status = OpenPaymentStatus::Commit(commit.clone(), sending.fees);
                    update_send_compact_state(compact_state, server_state, user_sender).await?;

                    // Send commit to user:
                    let payment_commit = PaymentCommit {
                        payment_id,
                        commit: commit.into(),
                    };
                    let compact_to_user = CompactToUser::PaymentCommit(payment_commit);
                    user_sender
                        .send(CompactToUserAck::CompactToUser(compact_to_user))
                        .await
                        .map_err(|_| CompactNodeError::UserSenderError)?;
                }
                (RequestResult::Failure, _) | (RequestResult::Success, true) => {
                    // Set payment as failed:
                    let ack_uid = compact_gen.gen_uid();
                    open_payment.status = OpenPaymentStatus::Failure(ack_uid.clone());
                    update_send_compact_state(compact_state, server_state, user_sender).await?;

                    // Inform the user about failure.
                    // Send a message about payment done:
                    let payment_done = PaymentDone {
                        payment_id: payment_id.clone(),
                        status: PaymentDoneStatus::Failure(ack_uid),
                    };
                    let compact_to_user = CompactToUser::PaymentDone(payment_done);
                    user_sender
                        .send(CompactToUserAck::CompactToUser(compact_to_user))
                        .await
                        .map_err(|_| CompactNodeError::UserSenderError)?;
                }
                (RequestResult::Success, false) => {
                    // There are still pending transactions. We will have to wait for the next
                    // transactions to complete.
                }
            };
        }
        AppServerToApp::ResponseClosePayment(response_close_payment) => {
            let mut compact_state = server_state.compact_state().clone();
            let open_payment = if let Some(open_payment) = compact_state
                .open_payments
                .get_mut(&response_close_payment.payment_id)
            {
                open_payment
            } else {
                warn!(
                    "ResponseClosePayment: Unrecognized payment_id: {:?}",
                    response_close_payment.payment_id
                );
                ack_close_payment(&response_close_payment, compact_gen, app_sender).await?;
                return Ok(());
            };

            match open_payment.clone().status {
                OpenPaymentStatus::SearchingRoute(_) | OpenPaymentStatus::FoundRoute(_) => {
                    warn!("ResponseClosePayment: Node closed payment before we opened it! payment_id {:?}", response_close_payment.payment_id);
                }
                OpenPaymentStatus::Sending(_sending) => {
                    // We expect failure here. It is not likely that the payment was successful if
                    // we never got a commit to hand to the seller.
                    let ack_uid = compact_gen.gen_uid();
                    open_payment.status = OpenPaymentStatus::Failure(ack_uid.clone());
                    update_send_compact_state(compact_state, server_state, user_sender).await?;

                    // Inform the user about failure.
                    // Send a message about payment done:
                    let payment_done = PaymentDone {
                        payment_id: response_close_payment.payment_id.clone(),
                        status: PaymentDoneStatus::Failure(ack_uid),
                    };
                    let compact_to_user = CompactToUser::PaymentDone(payment_done);
                    user_sender
                        .send(CompactToUserAck::CompactToUser(compact_to_user))
                        .await
                        .map_err(|_| CompactNodeError::UserSenderError)?;
                }
                OpenPaymentStatus::Commit(_commit, fees) => {
                    // This is the most common state to get a `ResponseClosePayment`.
                    // We will now be able to know whether the payment succeeded (and we get a receipt),
                    // or failed.
                    match &response_close_payment.status {
                        PaymentStatus::PaymentNotFound | PaymentStatus::Canceled(_) => {
                            // Set payment to failure:
                            let ack_uid = compact_gen.gen_uid();
                            open_payment.status = OpenPaymentStatus::Failure(ack_uid.clone());
                            update_send_compact_state(compact_state, server_state, user_sender)
                                .await?;

                            // Inform the user about failure.
                            // Send a message about payment done:
                            let payment_done = PaymentDone {
                                payment_id: response_close_payment.payment_id.clone(),
                                status: PaymentDoneStatus::Failure(ack_uid),
                            };
                            let compact_to_user = CompactToUser::PaymentDone(payment_done);
                            user_sender
                                .send(CompactToUserAck::CompactToUser(compact_to_user))
                                .await
                                .map_err(|_| CompactNodeError::UserSenderError)?;
                        }
                        PaymentStatus::Success(success) => {
                            // Set payment to success:
                            let ack_uid = compact_gen.gen_uid();
                            open_payment.status = OpenPaymentStatus::Success(
                                success.receipt.clone(),
                                fees,
                                ack_uid.clone(),
                            );
                            update_send_compact_state(compact_state, server_state, user_sender)
                                .await?;

                            // Inform the user about success.
                            // Send a message about payment done:
                            let payment_done = PaymentDone {
                                payment_id: response_close_payment.payment_id.clone(),
                                status: PaymentDoneStatus::Success(
                                    success.receipt.clone(),
                                    fees,
                                    ack_uid,
                                ),
                            };
                            let compact_to_user = CompactToUser::PaymentDone(payment_done);
                            user_sender
                                .send(CompactToUserAck::CompactToUser(compact_to_user))
                                .await
                                .map_err(|_| CompactNodeError::UserSenderError)?;
                        }
                    };
                }
                OpenPaymentStatus::Failure(_) => {
                    // This could happen if one of our transactions failed, an we already set the
                    // status to failed ourselves.
                }
                OpenPaymentStatus::Success(_, _, _) => {
                    warn!("ResponseClosePayment: Node sent ResponseClosePayment more than once! payment_id {:?}", response_close_payment.payment_id);
                }
            }
            // In any case, we acknowledge the `ResponseClosePayment` message, to make sure the
            // done payment is not stuck forever inside the node:
            ack_close_payment(&response_close_payment, compact_gen, app_sender).await?;
        }
        AppServerToApp::ReportMutations(report_mutations) => {
            // Save the original `node_report`:
            let mut node_report = server_state.node_report().clone();

            // Apply mutations to `node_report`:
            for mutation in &report_mutations.mutations {
                node_report
                    .mutate(mutation)
                    .map_err(|_| CompactNodeError::ReportMutationError)?;
            }

            // If `node_report` has changed, send it to the user:
            if &node_report != server_state.node_report() {
                server_state.update_node_report(node_report.clone());

                let compact_report = create_compact_report(
                    server_state.compact_state().clone(),
                    node_report.clone(),
                );
                let compact_to_user = CompactToUser::Report(compact_report);
                user_sender
                    .send(CompactToUserAck::CompactToUser(compact_to_user))
                    .await
                    .map_err(|_| CompactNodeError::UserSenderError)?;
            }

            // Possibly send acknowledgement for a completed command:
            if let Some(app_request_id) = report_mutations.opt_app_request_id {
                user_sender
                    .send(CompactToUserAck::Ack(app_request_id))
                    .await
                    .map_err(|_| CompactNodeError::UserSenderError)?;
            }
        }
        AppServerToApp::ResponseRoutes(mut client_response_routes) => {
            // Search for the corresponding OpenPayment:
            let mut compact_state = server_state.compact_state().clone();
            let mut opt_invoice_id_open_payment = None;
            for (payment_id, open_payment) in &mut compact_state.open_payments {
                if let OpenPaymentStatus::SearchingRoute(request_routes_id) =
                    &mut open_payment.status
                {
                    if request_routes_id == &mut client_response_routes.request_id {
                        opt_invoice_id_open_payment = Some((payment_id.clone(), open_payment));
                    }
                }
            }

            let (payment_id, open_payment) =
                if let Some(invoice_id_open_payment) = opt_invoice_id_open_payment {
                    invoice_id_open_payment
                } else {
                    // We don't remember this request
                    warn!(
                        "ResponseRoutes: Unrecognized request_routes_id: {:?}",
                        client_response_routes.request_id
                    );
                    return Ok(());
                };

            let (multi_route, multi_route_choice, fees) = if let Some(inner) =
                obtain_multi_route(&client_response_routes, open_payment.dest_payment)
            {
                inner
            } else {
                // A suitable route was not found.

                // Set payment as failure:
                let open_payment = compact_state.open_payments.get_mut(&payment_id).unwrap();
                let ack_uid = compact_gen.gen_uid();
                open_payment.status = OpenPaymentStatus::Failure(ack_uid);
                update_send_compact_state(compact_state, server_state, user_sender).await?;

                // Notify user that the payment has failed:
                let payment_fees = PaymentFees {
                    payment_id,
                    response: PaymentFeesResponse::Unreachable,
                };
                let compact_to_user = CompactToUser::PaymentFees(payment_fees);
                return user_sender
                    .send(CompactToUserAck::CompactToUser(compact_to_user))
                    .await
                    .map_err(|_| CompactNodeError::UserSenderError);
            };

            // Update compact state (keep the best multiroute):
            let confirm_id = compact_gen.gen_uid();
            let found_route = OpenPaymentStatusFoundRoute {
                confirm_id: confirm_id.clone(),
                multi_route,
                multi_route_choice,
                fees,
            };
            open_payment.status = OpenPaymentStatus::FoundRoute(found_route);
            update_send_compact_state(compact_state, server_state, user_sender).await?;

            // Notify user that a route was found (Send required fees):
            let payment_fees = PaymentFees {
                payment_id,
                response: PaymentFeesResponse::Fees(fees, confirm_id),
            };
            let compact_to_user = CompactToUser::PaymentFees(payment_fees);
            user_sender
                .send(CompactToUserAck::CompactToUser(compact_to_user))
                .await
                .map_err(|_| CompactNodeError::UserSenderError)?;
        }
    }
    Ok(())
}
