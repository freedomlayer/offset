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



/// A wrapper for a connector.
/// Always connects to the same address.
#[derive(Clone)]
pub struct ConstAddressConnector<C,A> {
    connector: C,
    address: A,
}

impl<C,A> ConstAddressConnector<C,A> {
    pub fn new(connector: C, address: A) -> ConstAddressConnector<C,A> {
        ConstAddressConnector {
            connector,
            address,
        }
    }
}


impl<C,A> Connector for ConstAddressConnector<C,A>
where
    C: Connector<Address=A>,
    A: Clone,
{
    type Address = ();
    type SendItem = C::SendItem;
    type RecvItem = C::RecvItem;

    fn connect(&mut self, _address: ()) 
        -> BoxFuture<'_, Option<ConnPair<C::SendItem, C::RecvItem>>> {
        self.connector.connect(self.address.clone())
    }
}
