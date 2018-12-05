use core::pin::Pin;
use futures::channel::mpsc;
use futures::Future;

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

pub type ConnPair<SendItem, RecvItem> = (mpsc::Sender<SendItem>,mpsc::Receiver<RecvItem>);

/// connect to a remote entity
pub trait Connector {
    type Address;
    type SendItem;
    type RecvItem;

    fn connect(&mut self, address: Self::Address) 
        -> BoxFuture<'_, Option<ConnPair<Self::SendItem, Self::RecvItem>>>;
}

/// Listen to connections from remote entities
pub trait Listener {
    type Connection;
    type Config;
    type Arg;

    fn listen(self, arg: Self::Arg) -> (mpsc::Sender<Self::Config>, 
                             mpsc::Receiver<Self::Connection>);
}

/// Transform a connection into another connection
pub trait ConnTransform {
    type OldSendItem;
    type OldRecvItem;
    type NewSendItem;
    type NewRecvItem;

    fn transform(&mut self, conn_pair: ConnPair<Self::OldSendItem, Self::OldRecvItem>) 
        -> BoxFuture<'_, Option<ConnPair<Self::NewSendItem, Self::NewRecvItem>>>;
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
