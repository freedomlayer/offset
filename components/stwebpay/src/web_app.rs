use std::net::SocketAddr;

use serde::{Deserialize, Serialize};

use http::{Request, Response, StatusCode};

use tide::{error::ResultExt, forms::ContextExt, Context, EndpointResult}; /*, response, App};*/

use app::gen::gen_payment_id;
use app::invoice::InvoiceId;
use app::route::{safe_multi_route_amounts, MultiRoute, MultiRouteChoice};
use app::ser_string::string_to_public_key;
use app::{AppBuyer, AppRoutes, MultiCommit, PublicKey};

struct AppState {
    local_public_key: PublicKey,
    app_buyer: AppBuyer,
    app_routes: AppRoutes,
    listen_addr: SocketAddr,
}

impl AppState {
    fn new(
        local_public_key: PublicKey,
        app_buyer: AppBuyer,
        app_routes: AppRoutes,
        listen_addr: SocketAddr,
    ) -> Self {
        AppState {
            local_public_key,
            app_buyer,
            app_routes,
            listen_addr,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct PaymentRequest {
    /// Description: What are we paying for?
    description: String,
    /// Id number of the invoice we are paying
    invoice_id: String,
    /// Destination public key to receive the payment
    dest_public_key: String,
    /// Total amount of credits we are requested to pay
    dest_payment: u128,
    /// URL the user should end up on in case the payment was successful
    success_url: String,
    /// URL the user should end up on in case the payment failed.
    failure_url: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct ProcessRequest {
    /// Id number of the invoice we are paying
    invoice_id: String,
    /// Destination public key to receive the payment
    dest_public_key: String,
    /// Total amount of credits we are requested to pay
    dest_payment: u128,
    /// URL the user should end up on in case the payment was successful
    success_url: String,
    /// URL the user should end up on in case the payment failed.
    failure_url: String,
}

#[derive(Debug)]
enum WebPaymentError {
    AppRoutesError,
    NoSuitableRoute,
}

/// Choose a route for pushing `amount` credits
pub fn choose_multi_route(
    multi_routes: &[MultiRoute],
    amount: u128,
) -> Option<(usize, MultiRouteChoice)> {
    // We naively select the first multi-route we find suitable:
    // TODO: Possibly improve this later:
    for (i, multi_route) in multi_routes.iter().enumerate() {
        if let Some(multi_route_choice) = safe_multi_route_amounts(multi_route, amount) {
            return Some((i, multi_route_choice));
        }
    }
    None
}

/// Calculate total fees for paying through a multi route
/// With the given choices for payments through routes
fn multi_route_fees(
    multi_route: &MultiRoute,
    multi_route_choice: &MultiRouteChoice,
) -> Option<u128> {
    let mut total_fees = 0u128;
    for (route_index, dest_payment) in multi_route_choice.iter() {
        let fee = multi_route.routes[*route_index]
            .rate
            .calc_fee(*dest_payment)
            .unwrap();
        total_fees = total_fees.checked_add(fee)?;
    }
    Some(total_fees)
}

async fn get_multi_route(
    mut app_routes: AppRoutes,
    dest_payment: u128,
    local_public_key: PublicKey,
    dest_public_key: PublicKey,
) -> Result<(MultiRoute, MultiRouteChoice), WebPaymentError> {
    let multi_routes = app_routes
        .request_routes(
            dest_payment,
            local_public_key, // source
            dest_public_key.clone(),
            None,
        ) // No exclusion of edges
        .await
        .map_err(|_| WebPaymentError::AppRoutesError)?;

    let (multi_route_index, multi_route_choice) =
        choose_multi_route(&multi_routes, dest_payment).ok_or(WebPaymentError::NoSuitableRoute)?;
    let multi_route = &multi_routes[multi_route_index];

    Ok((multi_route.clone(), multi_route_choice))
}

/// - Show user information about the payment
/// - Give user possible options:
///      - Ok
///      - Cancel
/// - Result is sent to "/process".
async fn payment(mut cx: Context<AppState>) -> EndpointResult<String> {
    let payment_request: PaymentRequest = cx.body_form().await?;

    /*
    // TODO: Present the expected payment fees to the user:
    let (multi_route, multi_route_choice) = get_multi_route(cx.state().app_routes.clone(),
                    payment_request.dest_payment,
                    cx.state().local_public_key.clone(),
                    dest_public_key).await;
                    */

    // TODO: How to do this concatenation safely?
    let process_url = format!("{}/process", cx.state().listen_addr);
    let response = format!(
        r###"
        <!DOCTYPE html>
        <html>
        <body>

        You are going to pay {dest_payment:}:

        <form method="post" action="{process_url:}">
          <input type="hidden" name="invoice_id" value="{invoice_id:?}">
          <input type="hidden" name="dest_public_key" value="{dest_public_key:}">
          <input type="hidden" name="dest_payment" value="{dest_payment:}">
          <input type="hidden" name="success_url" value="{success_url:?}">
          <input type="hidden" name="failure_url" value="{failure_url:?}">
          <button type="submit">
             Ok
          </button>
        </form>

        <form method="post" action="{failure_url:}">
          <button type="submit">
            Cancel
          </button>
        </form>

        </body>
        </html> 
    "###,
        process_url = process_url,
        invoice_id = payment_request.invoice_id,
        dest_public_key = payment_request.dest_public_key,
        dest_payment = payment_request.dest_payment,
        success_url = payment_request.success_url,
        failure_url = payment_request.failure_url
    );

    Ok(response)
}

#[derive(Debug)]
enum PayInvoiceError {
    AppRoutesError,
    NoSuitableRoute,
}

async fn pay_invoice(
    app_buyer: AppBuyer,
    app_routes: AppRoutes,
    invoice_id: InvoiceId,
    dest_public_key: PublicKey,
    dest_payment: u128,
) -> Result<(), PayInvoiceError> {
    // TODO: Get routes

    /*
    // Create a new payment
    let payment_id = gen_payment_id();
    let payment = Payment { payment_id };

    // Keep payment id for later reference:
    store_payment_to_file(&payment, &payment_file).map_err(|_| BuyerError::StorePaymentError)?;

    await!(app_buyer.create_payment(
        payment_id,
        invoice_id.clone(),
        dest_payment,
        dest_public_key.clone()
    ))
    .map_err(|_| BuyerError::CreatePaymentFailed)?;

    // Create new transactions (One for every route). On the first failure cancel all
    // transactions. Succeed only if all transactions succeed.

    let mut fut_list = Vec::new();
    for (route_index, dest_payment) in &multi_route_choice {
        let route = &multi_route.routes[*route_index];
        let request_id = gen_uid();
        let mut c_app_buyer = app_buyer.clone();
        fut_list.push(
            // TODO: Possibly a more efficient way than Box::pin?
            Box::pin(async move {
                await!(c_app_buyer.create_transaction(
                    payment_id,
                    request_id,
                    route.route.clone(),
                    *dest_payment,
                    route.rate.calc_fee(*dest_payment).unwrap(),
                ))
            }),
        );
    }

    let mut commits = Vec::new();
    for _ in 0..multi_route_choice.len() {
        let (output, _fut_index, new_fut_list) = await!(select_all(fut_list));
        match output {
            Ok(commit) => commits.push(commit),
            Err(_) => {
                let _ = await!(app_buyer.request_close_payment(payment_id));
                return Err(BuyerError::CreateTransactionFailed);
            }
        }
        fut_list = new_fut_list;
    }

    let _ = await!(app_buyer.request_close_payment(payment_id));

    let multi_commit = MultiCommit {
        invoice_id: invoice.invoice_id.clone(),
        total_dest_payment: invoice.dest_payment,
        commits,
    };

    writeln!(writer, "Payment successful!").map_err(|_| BuyerError::WriteError)?;

    // Store MultiCommit to file:
    store_multi_commit_to_file(&multi_commit, &commit_file)
        .map_err(|_| BuyerError::StoreCommitError)?;
    */
    unimplemented!();
}

/*
async fn process(mut cx: Context<AppState>) -> EndpointResult<String> {
    /*
    let buyer = cx.app_data().buyer.clone();
    let routes = cx.app_data().routes.clone();

    let invoice_id = string_to_invoice_id(&process_request.invoice_id);
    let dest_public_key = string_to_public_key(&process_request.dest_public_key).client_err()?;
    let dest_payment = process_request.dest_payment;

    pay_invoice(buyer, routes, process_request).await;

    // TODJK
    let process_request: ProcessRequest = cx.body_form().await?;


    let invoice =
        load_invoice_from_file(&invoice_file).map_err(|_| BuyerError::LoadInvoiceError)?;



    // Redirect to success url:
    let response = Response::builder()
        .status(StatusCode::FOUND)
        .header("Location", process_request.success_url)
        .body(());
    */

    unimplemented!();
}
*/

pub async fn serve_app(
    local_public_key: PublicKey,
    buyer: AppBuyer,
    routes: AppRoutes,
    listen_addr: SocketAddr,
) -> Result<(), std::io::Error> {
    let app_state = AppState::new(local_public_key, buyer, routes, listen_addr);

    let mut app = tide::App::with_state(app_state);
    app.at("/payment").post(payment);
    // app.at("/process").post(process);
    app.serve(listen_addr).await
}
