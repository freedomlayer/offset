use core::pin::Pin;
use futures::channel::{mpsc, oneshot};
use futures::{Future, SinkExt};
use crate::conn::{Connector, ConnPair};


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

impl<SI,RI,A> Connector for DummyConnector<SI,RI,A> 
where
    SI: Send,
    RI: Send,
    A: Send + Sync,
{
    type Address = A;
    type SendItem = SI;
    type RecvItem = RI;

    fn connect<'a>(&'a mut self, address: A) -> Pin<Box<dyn Future<Output=Option<ConnPair<Self::SendItem, Self::RecvItem>>> + Send + 'a>> {
        let (response_sender, response_receiver) = oneshot::channel();
        let conn_request = ConnRequest {
            address,
            response_sender,
        };

        let fut_conn_pair = async move {
            await!(self.req_sender.send(conn_request)).unwrap();
            await!(response_receiver).unwrap()
        };
        Box::pinned(fut_conn_pair)
    }
}

