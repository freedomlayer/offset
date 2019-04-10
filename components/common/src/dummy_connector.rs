use crate::conn::{BoxFuture, FutTransform};
use futures::channel::{mpsc, oneshot};
use futures::SinkExt;

pub struct ConnRequest<A, O> {
    pub address: A,
    response_sender: oneshot::Sender<O>,
}

impl<A, O> ConnRequest<A, O> {
    pub fn reply(self, response: O) {
        self.response_sender.send(response).ok().unwrap();
    }
}

/// A connector that contains only one pre-created connection.
pub struct DummyConnector<A, O> {
    req_sender: mpsc::Sender<ConnRequest<A, O>>,
}

impl<A, O> DummyConnector<A, O> {
    pub fn new(req_sender: mpsc::Sender<ConnRequest<A, O>>) -> Self {
        DummyConnector { req_sender }
    }
}

// #[derive(Clone)] does not work for DummyListener when compiling index_client
// Seems like it has a problem with having config_receiver inside ListenRequest.
// O is Option<(Sender<IndexClientToServer>, Receiver<IndexServerToClient>)>
// where Sender and Receiver are from futures.
// This is a workaround for this issue:
impl<A, O> Clone for DummyConnector<A, O> {
    fn clone(&self) -> DummyConnector<A, O> {
        DummyConnector {
            req_sender: self.req_sender.clone(),
        }
    }
}

impl<A, O> FutTransform for DummyConnector<A, O>
where
    O: Send,
    A: Send + Sync,
{
    type Input = A;
    type Output = O;

    fn transform(&mut self, address: A) -> BoxFuture<'_, Self::Output> {
        let (response_sender, response_receiver) = oneshot::channel();
        let conn_request = ConnRequest {
            address,
            response_sender,
        };

        let fut_conn_pair = async move {
            await!(self.req_sender.send(conn_request)).unwrap();
            await!(response_receiver).unwrap()
        };
        Box::pin(fut_conn_pair)
    }
}
