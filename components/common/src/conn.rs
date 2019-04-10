use core::pin::Pin;
use futures::channel::mpsc;
use futures::Future;
use std::marker::PhantomData;

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

pub type ConnPair<SendItem, RecvItem> = (mpsc::Sender<SendItem>, mpsc::Receiver<RecvItem>);
pub type ConnPairVec = ConnPair<Vec<u8>, Vec<u8>>;

/*
/// connect to a remote entity
pub trait Connector {
    type Address;
    type SendItem;
    type RecvItem;

    fn connect(&mut self, address: Self::Address)
        -> BoxFuture<'_, Option<ConnPair<Self::SendItem, Self::RecvItem>>>;
}
*/

/// Listen to connections from remote entities
pub trait Listener {
    type Connection;
    type Config;
    type Arg;

    fn listen(
        self,
        arg: Self::Arg,
    ) -> (mpsc::Sender<Self::Config>, mpsc::Receiver<Self::Connection>);
}

/// Apply a futuristic function over an input. Returns a boxed future that resolves
/// to type Output.
///
/// Ideally, we would have used `FnMut(Input) -> BoxFuture<'_, Output>`,
/// but implementing FnMut requires first that FnOnce will be implemented, and due to syntactic
/// lifetime issues we didn't find a way to implement it. See also:
///
/// https://users.rust-lang.org/t/implementing-fnmut-with-lifetime/2620
/// https://stackoverflow.com/q/32219798
///
pub trait FutTransform {
    type Input;
    type Output;

    fn transform(&mut self, input: Self::Input) -> BoxFuture<'_, Self::Output>;
}

/// A wrapper for a FutTransform that always gives the same input
#[derive(Clone)]
pub struct ConstFutTransform<FT, I> {
    fut_transform: FT,
    input: I,
}

impl<FT, I> ConstFutTransform<FT, I> {
    pub fn new(fut_transform: FT, input: I) -> ConstFutTransform<FT, I> {
        ConstFutTransform {
            fut_transform,
            input,
        }
    }
}

impl<FT, I, O> FutTransform for ConstFutTransform<FT, I>
where
    FT: FutTransform<Input = I, Output = O>,
    I: Clone,
{
    type Input = ();
    type Output = O;

    fn transform(&mut self, _input: ()) -> BoxFuture<'_, Self::Output> {
        self.fut_transform.transform(self.input.clone())
    }
}

/// Wraps an FnMut type in a type that implements FutTransform.
/// This could help mocking a FutTransform with a simple non futuristic function.
pub struct FuncFutTransform<F, I, O> {
    func: F,
    phantom_i: PhantomData<I>,
    phantom_o: PhantomData<O>,
}

// FuncFutTransform can be Clone only when F is Clone
impl<F, I, O> Clone for FuncFutTransform<F, I, O>
where
    F: Clone,
{
    fn clone(&self) -> FuncFutTransform<F, I, O> {
        FuncFutTransform {
            func: self.func.clone(),
            phantom_i: self.phantom_i,
            phantom_o: self.phantom_o,
        }
    }
}

impl<F, I, O> FuncFutTransform<F, I, O>
where
    F: FnMut(I) -> BoxFuture<'static, O>,
    O: Send,
{
    pub fn new(func: F) -> FuncFutTransform<F, I, O> {
        FuncFutTransform {
            func,
            phantom_i: PhantomData,
            phantom_o: PhantomData,
        }
    }
}

impl<F, I, O> FutTransform for FuncFutTransform<F, I, O>
where
    F: FnMut(I) -> BoxFuture<'static, O>,
    O: Send,
{
    type Input = I;
    type Output = O;

    fn transform(&mut self, input: Self::Input) -> BoxFuture<'_, Self::Output> {
        (self.func)(input)
    }
}
