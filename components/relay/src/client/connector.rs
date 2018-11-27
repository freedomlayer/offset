use core::pin::Pin;
use futures::channel::mpsc;
use futures::Future;

pub struct ConnPair<SendItem, RecvItem> {
    pub sender: mpsc::Sender<SendItem>,
    pub receiver: mpsc::Receiver<RecvItem>,
}

pub trait Connector {
    type Address;
    type SendItem;
    type RecvItem;
    fn connect<'a>(&'a mut self, address: Self::Address) 
        -> Pin<Box<dyn Future<Output=Option<ConnPair<Self::SendItem, Self::RecvItem>>> + Send + 'a>>;
}
