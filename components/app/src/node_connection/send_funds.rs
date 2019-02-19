use futures::{StreamExt, SinkExt};
use futures::channel::mpsc;
use common::multi_consumer::MultiConsumerClient;

use crypto::uid::Uid;
use crypto::crypto_rand::CryptoRandom;
use crypto::identity::PublicKey;

use proto::app_server::messages::{AppToAppServer, AppRequest};
use proto::funder::messages::{ResponseReceived, ResponseSendFundsResult,
    FriendsRoute, InvoiceId, UserRequestSendFunds, ReceiptAck, Receipt};

#[derive(Debug)]
pub enum SendFundsError {
    /// A local error occured when trying to send funds.
    /// (Connectivity error)
    LocalError,
    /// A remote error occured when trying to send funds.
    /// (Not enough credits, Some node cancelled along the route)
    RemoteError(PublicKey),
    /// The request was issued, but no response was received.  
    /// The request should be saved (By the caller) and resent at another time.
    NoResponse,
}

#[derive(Debug)]
pub struct ReceiptAckError;

#[derive(Clone)]
pub struct AppSendFunds<R> {
    sender: mpsc::Sender<AppToAppServer>,
    send_funds_mc: MultiConsumerClient<ResponseReceived>,
    done_app_requests_mc: MultiConsumerClient<Uid>,
    rng: R,
}

impl<R> AppSendFunds<R> 
where
    R: CryptoRandom,
{
    pub fn new(sender: mpsc::Sender<AppToAppServer>,
               send_funds_mc: MultiConsumerClient<ResponseReceived>,
               done_app_requests_mc: MultiConsumerClient<Uid>,
               rng: R) -> Self {

        AppSendFunds {
            sender,
            send_funds_mc,
            done_app_requests_mc,
            rng,
        }
    }

    pub async fn request_send_funds(&mut self, 
                                    request_id: Uid,
                                    route: FriendsRoute,
                                    invoice_id: InvoiceId,
                                    dest_payment: u128) -> Result<Receipt, SendFundsError> {

        let user_request_send_funds = UserRequestSendFunds {
            request_id,
            route,
            invoice_id,
            dest_payment,
        };
        let app_request_id = Uid::new(&self.rng);
        let to_app_server = AppToAppServer::new(app_request_id, 
                                                AppRequest::RequestSendFunds(user_request_send_funds));

        let mut incoming_send_funds = await!(self.send_funds_mc.request_stream())
            .map_err(|_| SendFundsError::LocalError)?;

        await!(self.sender.send(to_app_server))
            .map_err(|_| SendFundsError::LocalError)?;

        while let Some(response_received) = await!(incoming_send_funds.next()) {
            if response_received.request_id != request_id {
                // This is not our request:
                error!("Received a ResponseReceived for unknown request_id: {:?}", request_id);
                continue;
            }
            match response_received.result {
                ResponseSendFundsResult::Success(receipt) => 
                    return Ok(receipt),
                ResponseSendFundsResult::Failure(public_key) => 
                    return Err(SendFundsError::RemoteError(public_key)),
            }
        }

        // We lost connectivity before we got any response for the request to send funds.
        Err(SendFundsError::NoResponse)
    }

    pub async fn receipt_ack<'a>(&'a mut self, 
                             request_id: Uid, 
                             receipt: &'a Receipt) -> Result<(), ReceiptAckError> {

        let receipt_ack = ReceiptAck {
            request_id,
            receipt_signature: receipt.signature.clone(),
        };

        let app_request_id = Uid::new(&self.rng);
        let to_app_server = AppToAppServer::new(app_request_id, 
                                                AppRequest::ReceiptAck(receipt_ack));

        // Start listening to done requests:
        let mut incoming_done_requests = await!(self.done_app_requests_mc.request_stream())
            .map_err(|_| ReceiptAckError)?;

        // Send ReceiptAck:
        await!(self.sender.send(to_app_server))
            .map_err(|_| ReceiptAckError)?;

        // Wait for a sign that our request was received:
        while let Some(done_request_id) = await!(incoming_done_requests.next()) {
            if app_request_id == done_request_id {
                return Ok(())
            }
        }
        Err(ReceiptAckError)
    }
}
