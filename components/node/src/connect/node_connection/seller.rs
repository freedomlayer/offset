use common::multi_consumer::MultiConsumerClient;
use futures::channel::mpsc;
use futures::{SinkExt, StreamExt};

use crypto::invoice_id::InvoiceId;
use crypto::rand::{CryptoRandom, OffstSystemRandom};
use crypto::uid::Uid;

use proto::app_server::messages::{AppRequest, AppToAppServer};
use proto::funder::messages::{AddInvoice, MultiCommit};

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
        total_dest_payment: u128,
    ) -> Result<(), SellerError> {
        let app_request_id = Uid::new(&self.rng);
        let add_invoice = AddInvoice {
            invoice_id,
            total_dest_payment,
        };
        let to_app_server =
            AppToAppServer::new(app_request_id, AppRequest::AddInvoice(add_invoice));

        // Start listening to done requests:
        let mut incoming_done_requests = await!(self.done_app_requests_mc.request_stream())
            .map_err(|_| SellerError::ConnectivityError)?;

        // Send AddInvoice:
        await!(self.sender.send(to_app_server)).map_err(|_| SellerError::ConnectivityError)?;

        // Wait for a sign that our request was received:
        while let Some(done_request_id) = await!(incoming_done_requests.next()) {
            if app_request_id == done_request_id {
                return Ok(());
            }
        }
        // We lost connectivity before we got any response:
        Err(SellerError::NoResponse)
    }

    pub async fn cancel_invoice(&mut self, invoice_id: InvoiceId) -> Result<(), SellerError> {
        let app_request_id = Uid::new(&self.rng);
        let to_app_server =
            AppToAppServer::new(app_request_id, AppRequest::CancelInvoice(invoice_id));

        // Start listening to done requests:
        let mut incoming_done_requests = await!(self.done_app_requests_mc.request_stream())
            .map_err(|_| SellerError::ConnectivityError)?;

        // Send CancelInvoice:
        await!(self.sender.send(to_app_server)).map_err(|_| SellerError::ConnectivityError)?;

        // Wait for a sign that our request was received:
        while let Some(done_request_id) = await!(incoming_done_requests.next()) {
            if app_request_id == done_request_id {
                return Ok(());
            }
        }
        // We lost connectivity before we got any response:
        Err(SellerError::NoResponse)
    }

    pub async fn commit_invoice(&mut self, multi_commit: MultiCommit) -> Result<(), SellerError> {
        let app_request_id = Uid::new(&self.rng);
        let to_app_server =
            AppToAppServer::new(app_request_id, AppRequest::CommitInvoice(multi_commit));

        // Start listening to done requests:
        let mut incoming_done_requests = await!(self.done_app_requests_mc.request_stream())
            .map_err(|_| SellerError::ConnectivityError)?;

        // Send CommitInvoice:
        await!(self.sender.send(to_app_server)).map_err(|_| SellerError::ConnectivityError)?;

        // Wait for a sign that our request was received:
        while let Some(done_request_id) = await!(incoming_done_requests.next()) {
            if app_request_id == done_request_id {
                return Ok(());
            }
        }
        // We lost connectivity before we got any response:
        Err(SellerError::NoResponse)
    }
}
