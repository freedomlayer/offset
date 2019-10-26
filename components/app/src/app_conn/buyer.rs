use common::multi_consumer::MultiConsumerClient;
use futures::channel::mpsc;
use futures::{SinkExt, StreamExt};

use proto::crypto::{InvoiceId, PaymentId, PublicKey, Uid};

use crypto::rand::{CryptoRandom, OffstSystemRandom, RandGen};

use proto::app_server::messages::{AppRequest, AppToAppServer};
use proto::funder::messages::{
    AckClosePayment, Commit, CreatePayment, CreateTransaction, Currency, FriendsRoute,
    PaymentStatus, RequestResult, ResponseClosePayment, TransactionResult,
};

// TODO: Different in naming convention from AppConfigError and AppRoutesError:
#[derive(Debug)]
pub enum BuyerError {
    /// A local error occurred when trying to send funds.
    /// (Connectivity error)
    ConnectivityError,
    /// A remote error occurred when trying to send funds.
    /// (Not enough credits, Some node cancelled along the route)
    NodeError,
    /// The request was issued, but no response was received.
    /// The request should be saved (By the caller) and resent at another time.
    NoResponse,
}

#[derive(Clone)]
pub struct AppBuyer<R = OffstSystemRandom> {
    sender: mpsc::Sender<AppToAppServer>,
    transaction_results_mc: MultiConsumerClient<TransactionResult>,
    response_close_payments_mc: MultiConsumerClient<ResponseClosePayment>,
    done_app_requests_mc: MultiConsumerClient<Uid>,
    rng: R,
}

impl<R> AppBuyer<R>
where
    R: CryptoRandom,
{
    pub(super) fn new(
        sender: mpsc::Sender<AppToAppServer>,
        transaction_results_mc: MultiConsumerClient<TransactionResult>,
        response_close_payments_mc: MultiConsumerClient<ResponseClosePayment>,
        done_app_requests_mc: MultiConsumerClient<Uid>,
        rng: R,
    ) -> Self {
        AppBuyer {
            sender,
            transaction_results_mc,
            response_close_payments_mc,
            done_app_requests_mc,
            rng,
        }
    }

    pub async fn create_payment(
        &mut self,
        payment_id: PaymentId,
        invoice_id: InvoiceId,
        currency: Currency,
        total_dest_payment: u128,
        dest_public_key: PublicKey,
    ) -> Result<(), BuyerError> {
        let create_payment = CreatePayment {
            payment_id,
            invoice_id,
            currency,
            total_dest_payment,
            dest_public_key,
        };

        let app_request_id = Uid::rand_gen(&self.rng);
        let to_app_server = AppToAppServer::new(
            app_request_id.clone(),
            AppRequest::CreatePayment(create_payment),
        );

        // Start listening to done requests:
        let mut incoming_done_requests = self
            .done_app_requests_mc
            .request_stream()
            .await
            .map_err(|_| BuyerError::ConnectivityError)?;

        // Send CreatePayment:
        self.sender
            .send(to_app_server)
            .await
            .map_err(|_| BuyerError::ConnectivityError)?;

        // Wait for a sign that our request was received:
        while let Some(done_request_id) = incoming_done_requests.next().await {
            if app_request_id == done_request_id {
                return Ok(());
            }
        }
        // We lost connectivity before we got any response:
        Err(BuyerError::NoResponse)
    }

    pub async fn create_transaction(
        &mut self,
        payment_id: PaymentId,
        request_id: Uid,
        route: FriendsRoute,
        dest_payment: u128,
        fees: u128,
        // TODO: Possibly use a new type instead of `Option<Commit>` ?
    ) -> Result<Option<Commit>, BuyerError> {
        let create_transaction = CreateTransaction {
            payment_id,
            request_id: request_id.clone(),
            route,
            dest_payment,
            fees,
        };
        let app_request_id = Uid::rand_gen(&self.rng);
        let to_app_server = AppToAppServer::new(
            app_request_id,
            AppRequest::CreateTransaction(create_transaction),
        );

        let mut incoming_transaction_results = self
            .transaction_results_mc
            .request_stream()
            .await
            .map_err(|_| BuyerError::ConnectivityError)?;

        self.sender
            .send(to_app_server)
            .await
            .map_err(|_| BuyerError::ConnectivityError)?;

        while let Some(transaction_result) = incoming_transaction_results.next().await {
            if transaction_result.request_id != request_id {
                // This is not our request
                continue;
            }
            match transaction_result.result {
                RequestResult::Complete(commit) => return Ok(Some(commit)),
                RequestResult::Success => return Ok(None),
                RequestResult::Failure => return Err(BuyerError::NodeError),
            }
        }

        // We lost connectivity before we got any response:
        Err(BuyerError::NoResponse)
    }

    pub async fn request_close_payment(
        &mut self,
        payment_id: PaymentId,
    ) -> Result<PaymentStatus, BuyerError> {
        let app_request_id = Uid::rand_gen(&self.rng);
        let to_app_server = AppToAppServer::new(
            app_request_id,
            AppRequest::RequestClosePayment(payment_id.clone()),
        );

        let mut incoming_response_close_payment = self
            .response_close_payments_mc
            .request_stream()
            .await
            .map_err(|_| BuyerError::ConnectivityError)?;

        self.sender
            .send(to_app_server)
            .await
            .map_err(|_| BuyerError::ConnectivityError)?;

        while let Some(response_close_payment) = incoming_response_close_payment.next().await {
            if response_close_payment.payment_id != payment_id {
                // This is not our close request
                continue;
            }
            return Ok(response_close_payment.status);
        }

        // We lost connectivity before we got any response:
        Err(BuyerError::NoResponse)
    }

    pub async fn ack_close_payment(
        &mut self,
        payment_id: PaymentId,
        ack_uid: Uid,
    ) -> Result<(), BuyerError> {
        let app_request_id = Uid::rand_gen(&self.rng);
        let ack_close_payment = AckClosePayment {
            payment_id,
            ack_uid,
        };

        let to_app_server = AppToAppServer::new(
            app_request_id.clone(),
            AppRequest::AckClosePayment(ack_close_payment),
        );

        // Start listening to done requests:
        let mut incoming_done_requests = self
            .done_app_requests_mc
            .request_stream()
            .await
            .map_err(|_| BuyerError::ConnectivityError)?;

        // Send ReceiptAck:
        self.sender
            .send(to_app_server)
            .await
            .map_err(|_| BuyerError::ConnectivityError)?;

        // Wait for a sign that our request was received:
        while let Some(done_request_id) = incoming_done_requests.next().await {
            if app_request_id == done_request_id {
                return Ok(());
            }
        }

        // We lost connectivity before we got any response:
        Err(BuyerError::NoResponse)
    }
}
