use std::marker::PhantomData;
use std::sync::Arc;
use futures::channel::mpsc;
use futures::future::FutureObj;
use futures::{FutureExt, StreamExt};
use async_mutex::AsyncMutex;
use super::connector::{Connector, ConnPair};


/// A connector that contains only one pre-created connection.
#[derive(Clone)]
pub struct DummyConnector<SI,RI,A> {
    arc_mut_receiver: Arc<AsyncMutex<mpsc::Receiver<ConnPair<SI,RI>>>>,
    phantom_a: PhantomData<A>
}

impl<SI,RI,A> DummyConnector<SI,RI,A> {
    pub fn new(receiver: mpsc::Receiver<ConnPair<SI,RI>>) -> Self {
        DummyConnector { 
            arc_mut_receiver: Arc::new(AsyncMutex::new(receiver)),
            phantom_a: PhantomData,
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

    fn connect(&mut self, _address: A) -> FutureObj<Option<ConnPair<Self::SendItem, Self::RecvItem>>> {
        let fut_conn_pair = async move {
            let mut guard = await!(self.arc_mut_receiver.lock());
            await!(guard.next())
        };
        let future_obj = FutureObj::new(fut_conn_pair.boxed());
        future_obj
    }
}

