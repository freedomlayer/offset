use futures::future::FutureObj;
use futures::channel::mpsc;

pub struct ConnPair<SendItem, RecvItem> {
    pub sender: mpsc::Sender<SendItem>,
    pub receiver: mpsc::Receiver<RecvItem>,
}

pub trait Connector {
    type Address;
    type SendItem;
    type RecvItem;
    fn connect(&mut self, address: Self::Address) 
        -> FutureObj<Option<ConnPair<Self::SendItem, Self::RecvItem>>>;
}
