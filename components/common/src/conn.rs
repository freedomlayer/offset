use std::marker::PhantomData;
use core::pin::Pin;
use futures::channel::mpsc;
use futures::{future, Future};

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
    type Arg;

    fn transform(&mut self, arg: Self::Arg, conn_pair: ConnPair<Self::OldSendItem, Self::OldRecvItem>) 
        -> BoxFuture<'_, Option<ConnPair<Self::NewSendItem, Self::NewRecvItem>>>;
}


/// Apply a futuristic function over an input. Returns a boxed future that resolves
/// to type Output.
///
/// Idealy we would have used `FnMut(Input) -> BoxFuture<'_, Output>`,
/// but implementing FnMut requires first that FnOnce will be implemented, and due to syntactic
/// lifetime issues we didn't find a way to implement it. See also:
///
/// https://users.rust-lang.org/t/implementing-fnmut-with-lifetime/2620
/// https://stackoverflow.com/questions/32219798/
///             how-to-implement-fnmut-which-returns-reference-with-lifetime-parameter
///
pub trait FutTransform {
    type Input;
    type Output;

    fn transform(&mut self, input: Self::Input)
        -> BoxFuture<'_, Self::Output>;
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



/// The Identity connection transformation.
/// Returns exactly the same connection it has received.
#[derive(Clone)]
pub struct IdentityConnTransform<SI,RI,ARG> {
    phantom_send_item: PhantomData<SI>,
    phantom_recv_item: PhantomData<RI>,
    phantom_arg: PhantomData<ARG>,
}

impl<SI,RI,ARG> IdentityConnTransform<SI,RI,ARG> {
    pub fn new() -> IdentityConnTransform<SI,RI,ARG> {
        IdentityConnTransform {
            phantom_send_item: PhantomData,
            phantom_recv_item: PhantomData,
            phantom_arg: PhantomData,
        }
    }

}


impl<SI,RI,ARG> ConnTransform for IdentityConnTransform<SI,RI,ARG> 
where
    SI: Send,
    RI: Send,
{
    type OldSendItem = SI;
    type OldRecvItem = RI;
    type NewSendItem = SI;
    type NewRecvItem = RI;
    type Arg = ARG;

    fn transform(&mut self, _arg: ARG, conn_pair: ConnPair<SI,RI>) 
        -> BoxFuture<'_, Option<ConnPair<SI,RI>>> {

        Box::pinned(future::ready(Some(conn_pair)))
    }
}

