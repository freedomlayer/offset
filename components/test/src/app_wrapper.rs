use futures::sink::SinkExt;
use futures::stream::StreamExt;

use app::common::{Currency, FriendsRoute, MultiRoute, PaymentId, PaymentStatus, PublicKey, Uid};
use app::conn::{
    self, AppRequest, AppServerToApp, AppToAppServer, ConnPairApp, RequestResult,
    ResponseRoutesResult,
};
use app::gen::gen_uid;

#[derive(Debug)]
pub struct AppWrapperError;

/// Send a request and wait until the request is acked
pub async fn send_request(
    conn_pair: &mut ConnPairApp,
    app_request: AppRequest,
) -> Result<(), AppWrapperError> {
    let app_request_id = gen_uid();
    let app_to_app_server = AppToAppServer {
        app_request_id: app_request_id.clone(),
        app_request,
    };
    conn_pair
        .sender
        .send(app_to_app_server)
        .await
        .map_err(|_| AppWrapperError)?;

    // Wait until we get an ack for our request:
    while let Some(app_server_to_app) = conn_pair.receiver.next().await {
        if let AppServerToApp::ReportMutations(report_mutations) = app_server_to_app {
            if let Some(cur_app_request_id) = report_mutations.opt_app_request_id {
                if cur_app_request_id == app_request_id {
                    return Ok(());
                }
            }
        }
    }

    return Err(AppWrapperError);
}

pub async fn request_routes(
    conn_pair: &mut ConnPairApp,
    currency: Currency,
    dest_payment: u128,
    src_public_key: PublicKey,
    dest_public_key: PublicKey,
    opt_exclude: Option<(PublicKey, PublicKey)>,
) -> Result<Vec<MultiRoute>, AppWrapperError> {
    let request_routes_id = gen_uid();
    let app_request = conn::routes::request_routes(
        request_routes_id.clone(),
        currency,
        dest_payment,
        src_public_key,
        dest_public_key,
        opt_exclude,
    );

    // Note: We never use the randomly generated `app_request_id` later.
    // Instead, we are waiting
    // We generate it here just because we need to put some value into `app_request_id`.
    let app_to_app_server = AppToAppServer {
        app_request_id: gen_uid(),
        app_request,
    };
    conn_pair
        .sender
        .send(app_to_app_server)
        .await
        .map_err(|_| AppWrapperError)?;

    // Wait until we get back response routes:
    while let Some(app_server_to_app) = conn_pair.receiver.next().await {
        if let AppServerToApp::ResponseRoutes(client_response_routes) = app_server_to_app {
            if client_response_routes.request_id == request_routes_id {
                if let ResponseRoutesResult::Success(multi_routes) = client_response_routes.result {
                    return Ok(multi_routes);
                }
            }
        }
    }
    return Err(AppWrapperError);
}

/*
/// Request to close payment, but do not wait for the payment to be closed.
pub async fn request_close_payment_nowait(
    conn_pair: &mut ConnPairApp,
    payment_id: PaymentId,
) -> Result<(), AppWrapperError> {
    let app_request = conn::buyer::request_close_payment(payment_id.clone());
    let app_request_id = gen_uid();
    let app_to_app_server = AppToAppServer {
        // We don't really care about app_request_id here, as we can wait on `request_id`
        // instead.
        app_request_id: app_request_id.clone(),
        app_request,
    };

    conn_pair
        .sender
        .send(app_to_app_server)
        .await
        .map_err(|_| AppWrapperError)?;

    while let Some(app_server_to_app) = conn_pair.receiver.next().await {
        if let AppServerToApp::ReportMutations(report_mutations) = app_server_to_app {
            if let Some(cur_app_request_id) = report_mutations.opt_app_request_id {
                if cur_app_request_id == app_request_id {
                    return Ok(());
                }
            }
        }
    }

    return Err(AppWrapperError);
}
*/

/// Request to close the payment, and wait for the payment to be closed.
pub async fn request_close_payment(
    conn_pair: &mut ConnPairApp,
    payment_id: PaymentId,
) -> Result<PaymentStatus, AppWrapperError> {
    let app_request = conn::buyer::request_close_payment(payment_id.clone());
    let app_request_id = gen_uid();
    let app_to_app_server = AppToAppServer {
        // We don't really care about app_request_id here, as we can wait on `request_id`
        // instead.
        app_request_id: app_request_id.clone(),
        app_request,
    };

    conn_pair
        .sender
        .send(app_to_app_server)
        .await
        .map_err(|_| AppWrapperError)?;

    while let Some(app_server_to_app) = conn_pair.receiver.next().await {
        if let AppServerToApp::ResponseClosePayment(response_close_payment) = app_server_to_app {
            if payment_id == response_close_payment.payment_id {
                return Ok(response_close_payment.status);
            }
        }
    }

    return Err(AppWrapperError);
}

/// Request to close payment, but do not wait for the payment to be closed.
pub async fn ack_close_payment(
    conn_pair: &mut ConnPairApp,
    payment_id: PaymentId,
    ack_uid: Uid,
) -> Result<(), AppWrapperError> {
    let app_request = conn::buyer::ack_close_payment(payment_id.clone(), ack_uid.clone());
    let app_request_id = gen_uid();
    let app_to_app_server = AppToAppServer {
        // We don't really care about app_request_id here, as we can wait on `request_id`
        // instead.
        app_request_id: app_request_id.clone(),
        app_request,
    };

    conn_pair
        .sender
        .send(app_to_app_server)
        .await
        .map_err(|_| AppWrapperError)?;

    while let Some(app_server_to_app) = conn_pair.receiver.next().await {
        if let AppServerToApp::ReportMutations(report_mutations) = app_server_to_app {
            if let Some(cur_app_request_id) = report_mutations.opt_app_request_id {
                if cur_app_request_id == app_request_id {
                    return Ok(());
                }
            }
        }
    }

    return Err(AppWrapperError);
}

/// Create a transaction and wait for the transaction result
pub async fn create_transaction(
    conn_pair: &mut ConnPairApp,
    payment_id: PaymentId,
    request_id: Uid,
    route: FriendsRoute,
    dest_payment: u128,
    fees: u128,
) -> Result<RequestResult, AppWrapperError> {
    let app_request = conn::buyer::create_transaction(
        payment_id.clone(),
        request_id.clone(),
        route,
        dest_payment,
        fees,
    );
    let app_request_id = gen_uid();
    let app_to_app_server = AppToAppServer {
        // We don't really care about app_request_id here, as we can wait on `request_id`
        // instead.
        app_request_id: app_request_id.clone(),
        app_request,
    };

    conn_pair
        .sender
        .send(app_to_app_server)
        .await
        .map_err(|_| AppWrapperError)?;

    while let Some(app_server_to_app) = conn_pair.receiver.next().await {
        if let AppServerToApp::TransactionResult(transaction_result) = app_server_to_app {
            assert_eq!(request_id, transaction_result.request_id);
            if request_id == transaction_result.request_id {
                return Ok(transaction_result.result);
            }
        }
    }

    return Err(AppWrapperError);
}
