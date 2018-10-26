use futures::channel::{mpsc, oneshot};
use futures::future::FutureObj;
use futures::{FutureExt, SinkExt};
use super::connector::{Connector, ConnPair};


pub struct ConnRequest<SI,RI,A> {
    pub address: A,
    response_sender: oneshot::Sender<ConnPair<SI,RI>>,
}

impl<SI,RI,A> ConnRequest<SI,RI,A> {
    pub fn reply(self, conn_pair: ConnPair<SI,RI>) {
        self.response_sender.send(conn_pair).ok().unwrap();
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

    fn connect(&mut self, address: A) -> FutureObj<Option<ConnPair<Self::SendItem, Self::RecvItem>>> {
        let (response_sender, response_receiver) = oneshot::channel();
        let conn_request = ConnRequest {
            address,
            response_sender,
        };

        let fut_conn_pair = async move {
            await!(self.req_sender.send(conn_request)).unwrap();
            await!(response_receiver).ok()
        };
        let future_obj = FutureObj::new(fut_conn_pair.boxed());
        future_obj
    }
}

