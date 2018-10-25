use std::marker::PhantomData;
use futures::channel::mpsc;
use futures::future::FutureObj;
use futures::{FutureExt, StreamExt};
use super::connector::{Connector, ConnPair};


/// A connector that contains only one pre-created connection.
pub struct DummyConnector<SI,RI,A> {
    amutex_receiver: mpsc::Receiver<ConnPair<SI,RI>>,
    phantom_a: PhantomData<A>
}

impl<SI,RI,A> DummyConnector<SI,RI,A> {
    pub fn new(receiver: mpsc::Receiver<ConnPair<SI,RI>>) -> Self {
        DummyConnector { 
            amutex_receiver: receiver,
            phantom_a: PhantomData,
        }
    }
}

impl<SI,RI,A> Connector for DummyConnector<SI,RI,A> 
where
    SI: Send,
    RI: Send,
{
    type Address = A;
    type SendItem = SI;
    type RecvItem = RI;

    fn connect(&mut self, _address: A) -> FutureObj<Option<ConnPair<Self::SendItem, Self::RecvItem>>> {
        let fut_conn_pair = self.amutex_receiver.next();
        let future_obj = FutureObj::new(fut_conn_pair.boxed());
        future_obj
    }
}
