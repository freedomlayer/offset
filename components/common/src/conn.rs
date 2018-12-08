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


/// Wraps an FnMut type in a type that implements FutTransform.
/// This could help mocking a FutTransform with a simple non futuristic function.
pub struct FuncFutTransform<F,I,O> {
    func: F,
    phantom_i: PhantomData<I>,
    phantom_o: PhantomData<O>,
}

// It seems like deriving Clone automatically doesn't work,
// so we need to implement manually. Is this a bug in how #[derive(Clone)] works?
impl<F,I,O> Clone for FuncFutTransform<F,I,O> 
where
    F: Clone,
{
    fn clone(&self) -> FuncFutTransform<F,I,O> {
        FuncFutTransform {
            func: self.func.clone(),
            phantom_i: self.phantom_i.clone(),
            phantom_o: self.phantom_o.clone(),
        }
    }
}


impl<F,I,O> FuncFutTransform<F,I,O> 
where
    F: FnMut(I) -> O,
    O: Send,
{
    pub fn new(func: F) -> FuncFutTransform<F,I,O> {
        FuncFutTransform {
            func,
            phantom_i: PhantomData,
            phantom_o: PhantomData,
        }
    }
}

impl<F,I,O> FutTransform for FuncFutTransform<F,I,O> 
where
    F: FnMut(I) -> O,
    O: Send,
{
    type Input = I;
    type Output = O;

    fn transform(&mut self, input: Self::Input)
        -> BoxFuture<'_, Self::Output> {
        Box::pinned(future::ready((self.func)(input)))
    } 
}

