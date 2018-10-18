use futures::future::FutureObj;
use futures::channel::mpsc;

pub struct ConnPair<Item> {
    pub sender: mpsc::Sender<Item>,
    pub receiver: mpsc::Receiver<Item>,
}

pub trait Connector {
    type Address;
    type Item;
    fn connect(&mut self, address: Self::Address) -> FutureObj<Option<ConnPair<Self::Item>>>;
}

