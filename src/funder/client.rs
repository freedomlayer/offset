use futures::{Future, Sink};
use futures::sync::{mpsc, oneshot};

use proto::common::SendFundsReceipt;
use proto::funder::InvoiceId;
use super::messages::{RequestSendFunds, ResponseSendFunds, FriendsRoute};

#[derive(Debug)]
pub enum FunderClientError {
    RequestSendFailed,
    OneshotReceiverCanceled,
    SendFundsFailed,
}

#[derive(Clone)]
pub struct FunderClient {
    requests_sender: mpsc::Sender<RequestSendFunds>,
}


impl FunderClient {
    pub fn new(requests_sender: mpsc::Sender<RequestSendFunds>) -> Self {
        FunderClient { requests_sender }
    }
    /*
    pub fn request_send_funds(&self, route: FriendsRoute, invoice_id: InvoiceId, payment: u128) 
        -> impl Future<Item=SendFundsReceipt, Error=FunderClientError> {

        let rsender = self.requests_sender.clone();
        let (tx, rx) = oneshot::channel();
        let request = RequestSendFunds {
            route,
            invoice_id,
            payment,
            response_sender: tx,
        };
        rsender
         .send(request)
         .map_err(|_| FunderClientError::RequestSendFailed)
         .and_then(|_| rx.map_err(|oneshot::Canceled| FunderClientError::OneshotReceiverCanceled))
         .and_then(|response| {
             match response {
                 ResponseSendFunds::Failure => Err(FunderClientError::SendFundsFailed),
                 ResponseSendFunds::Success(receipt) => Ok(receipt),
             }
         })
    }
    */
}

