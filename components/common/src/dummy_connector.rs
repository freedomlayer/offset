use futures::channel::{mpsc, oneshot};
use futures::SinkExt;
use crate::conn::{FutTransform, ConnPair, BoxFuture};


pub struct ConnRequest<SI,RI,A> {
    pub address: A,
    response_sender: oneshot::Sender<Option<ConnPair<SI,RI>>>,
}

impl<SI,RI,A> ConnRequest<SI,RI,A> {
    pub fn reply(self, opt_conn_pair: Option<ConnPair<SI,RI>>) {
        self.response_sender.send(opt_conn_pair).ok().unwrap();
    }
}

/// A connector that contains only one pre-created connection.
#[derive(Clone)]
pub struct DummyConnector<SI,RI,A> {
    req_sender: mpsc::Sender<ConnRequest<SI,RI,A>>,
}

impl<SI,RI,A> DummyConnector<SI,RI,A> {
    pub fn new(req_sender: mpsc::Sender<ConnRequest<SI,RI,A>>) -> Self {
        DummyConnector { 
            req_sender,
        }
    }
}

impl<SI,RI,A> FutTransform for DummyConnector<SI,RI,A> 
where
    SI: Send,
    RI: Send,
    A: Send + Sync,
{
    type Input = A;
    type Output = Option<ConnPair<SI,RI>>;

    fn transform<'a>(&'a mut self, address: A) -> BoxFuture<'_, Self::Output> {
        let (response_sender, response_receiver) = oneshot::channel();
        let conn_request = ConnRequest {
            address,
            response_sender,
        };

        let fut_conn_pair = async move {
            await!(self.req_sender.send(conn_request)).unwrap();
            await!(response_receiver).ok()?
        };
        Box::pinned(fut_conn_pair)
    }
}

