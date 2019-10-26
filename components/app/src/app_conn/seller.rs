use common::multi_consumer::MultiConsumerClient;
use futures::channel::mpsc;
use futures::{SinkExt, StreamExt};

use proto::crypto::{InvoiceId, Uid};

use crypto::rand::{CryptoRandom, OffstSystemRandom, RandGen};

use proto::app_server::messages::{AppRequest, AppToAppServer};
use proto::funder::messages::{AddInvoice, Currency, Commit};

// TODO: Different in naming convention from AppConfigError and AppRoutesError:
#[derive(Debug)]
pub enum SellerError {
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
pub struct AppSeller<R = OffstSystemRandom> {
    sender: mpsc::Sender<AppToAppServer>,
    done_app_requests_mc: MultiConsumerClient<Uid>,
    rng: R,
}

impl<R> AppSeller<R>
where
    R: CryptoRandom,
{
    pub(super) fn new(
        sender: mpsc::Sender<AppToAppServer>,
        done_app_requests_mc: MultiConsumerClient<Uid>,
        rng: R,
    ) -> Self {
        AppSeller {
            sender,
            done_app_requests_mc,
            rng,
        }
    }

    pub async fn add_invoice(
        &mut self,
        invoice_id: InvoiceId,
        currency: Currency,
        total_dest_payment: u128,
    ) -> Result<(), SellerError> {
        let app_request_id = Uid::rand_gen(&self.rng);
        let add_invoice = AddInvoice {
            invoice_id,
            currency,
            total_dest_payment,
        };
        let to_app_server =
            AppToAppServer::new(app_request_id.clone(), AppRequest::AddInvoice(add_invoice));

        // Start listening to done requests:
        let mut incoming_done_requests = self
            .done_app_requests_mc
            .request_stream()
            .await
            .map_err(|_| SellerError::ConnectivityError)?;

        // Send AddInvoice:
        self.sender
            .send(to_app_server)
            .await
            .map_err(|_| SellerError::ConnectivityError)?;

        // Wait for a sign that our request was received:
        while let Some(done_request_id) = incoming_done_requests.next().await {
            if app_request_id == done_request_id {
                return Ok(());
            }
        }
        // We lost connectivity before we got any response:
        Err(SellerError::NoResponse)
    }

    pub async fn cancel_invoice(&mut self, invoice_id: InvoiceId) -> Result<(), SellerError> {
        let app_request_id = Uid::rand_gen(&self.rng);
        let to_app_server = AppToAppServer::new(
            app_request_id.clone(),
            AppRequest::CancelInvoice(invoice_id),
        );

        // Start listening to done requests:
        let mut incoming_done_requests = self
            .done_app_requests_mc
            .request_stream()
            .await
            .map_err(|_| SellerError::ConnectivityError)?;

        // Send CancelInvoice:
        self.sender
            .send(to_app_server)
            .await
            .map_err(|_| SellerError::ConnectivityError)?;

        // Wait for a sign that our request was received:
        while let Some(done_request_id) = incoming_done_requests.next().await {
            if app_request_id == done_request_id {
                return Ok(());
            }
        }
        // We lost connectivity before we got any response:
        Err(SellerError::NoResponse)
    }

    pub async fn commit_invoice(&mut self, commit: Commit) -> Result<(), SellerError> {
        let app_request_id = Uid::rand_gen(&self.rng);
        let to_app_server = AppToAppServer::new(
            app_request_id.clone(),
            AppRequest::CommitInvoice(commit),
        );

        // Start listening to done requests:
        let mut incoming_done_requests = self
            .done_app_requests_mc
            .request_stream()
            .await
            .map_err(|_| SellerError::ConnectivityError)?;

        // Send CommitInvoice:
        self.sender
            .send(to_app_server)
            .await
            .map_err(|_| SellerError::ConnectivityError)?;

        // Wait for a sign that our request was received:
        while let Some(done_request_id) = incoming_done_requests.next().await {
            if app_request_id == done_request_id {
                return Ok(());
            }
        }
        // We lost connectivity before we got any response:
        Err(SellerError::NoResponse)
    }
}
