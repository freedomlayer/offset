use core::pin::Pin;
use futures::channel::mpsc;
use futures::Future;

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

pub struct ConnPair<SendItem, RecvItem> {
    pub sender: mpsc::Sender<SendItem>,
    pub receiver: mpsc::Receiver<RecvItem>,
}

pub trait Connector {
    type Address;
    type SendItem;
    type RecvItem;
    fn connect(&mut self, address: Self::Address) 
        -> BoxFuture<'_, Option<ConnPair<Self::SendItem, Self::RecvItem>>>;
}
