extern crate futures;

use std::mem;
use std::collections::VecDeque;
use std::cell::RefCell;
use std::rc::Rc;

use self::futures::sync::oneshot;
use self::futures::future::IntoFuture;
use self::futures::{Future, Poll, Async};


enum AsyncMutexState<T> {
    Ready(T),
    Busy(VecDeque<oneshot::Sender<T>>),
    Empty,
}

pub struct AsyncMutex<T> {
    async_mutex_state: Rc<RefCell<AsyncMutexState<T>>>,
}


enum AcquireFutureState<T,F,G> {
    WaitItem((oneshot::Receiver<T>, F)),
    WaitFunc(G),
    Empty,
}

pub struct AcquireFuture<T,F,G> {
    async_mutex_state: Rc<RefCell<AsyncMutexState<T>>>,
    acquire_future_state: AcquireFutureState<T,F,G>,
}

#[derive(Debug)]
enum IoError {
    ReceiverCanceled,
    SendFailed,
}

#[derive(Debug)]
pub enum AsyncMutexError<E> {
    IoError(IoError),
    FuncError(E),
}

impl<T,F,B,G,E,O> Future for AcquireFuture<T,F,G> 
where
    F: FnOnce(T) -> B,
    G: Future<Item=(T,O), Error=E>,
    B: IntoFuture<Item=(T,O), Error=E, Future=G>
{
    type Item = O;
    type Error = AsyncMutexError<E>;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        loop {
            match mem::replace(&mut self.acquire_future_state, AcquireFutureState::Empty) {
                AcquireFutureState::Empty => unreachable!(),
                AcquireFutureState::WaitItem((mut receiver, fut_func)) => {
                    match receiver.poll() {
                        Ok(Async::Ready(t)) => {
                            self.acquire_future_state = 
                                AcquireFutureState::WaitFunc(fut_func(t).into_future());
                        },
                        Ok(Async::NotReady) => {
                            self.acquire_future_state = 
                                AcquireFutureState::WaitItem((receiver, fut_func));
                            return Ok(Async::NotReady);
                        },
                        Err(oneshot::Canceled) => {
                            return Err(AsyncMutexError::IoError(IoError::ReceiverCanceled));
                        },
                    }
                },
                AcquireFutureState::WaitFunc(mut fut_result) => {
                    match fut_result.poll() {
                        Ok(Async::NotReady) => return Ok(Async::NotReady),
                        Ok(Async::Ready((t, output))) => {
                            // We need to put the item back, and notify the next waiter on the
                            // queue that it is ready
                            let ref mut b_state_ref = *self.async_mutex_state.borrow_mut();
                            match mem::replace(b_state_ref, 
                                               AsyncMutexState::Empty) {

                                AsyncMutexState::Empty => unreachable!(),
                                AsyncMutexState::Ready(_) => unreachable!(),
                                AsyncMutexState::Busy(mut pending) => {
                                    if let Some(sender) = pending.pop_front() {
                                        match sender.send(t) {
                                            Ok(()) => *b_state_ref = AsyncMutexState::Busy(pending),
                                            Err(_t) => return Err(AsyncMutexError::IoError(IoError::SendFailed)),
                                        };
                                    
                                    } else {
                                        *b_state_ref = AsyncMutexState::Ready(t);
                                    }

                                }
                            }
                            return Ok(Async::Ready(output));
                        },
                        Err(e) => return Err(AsyncMutexError::FuncError(e)),
                    }
                },
            }
        }
    }
}


/// A futures based Mutex.
/// Allows to own an item from a few different futures.
impl<T> AsyncMutex<T> {
    pub fn new(t: T) -> Self {
        AsyncMutex {
            async_mutex_state: Rc::new(RefCell::new(AsyncMutexState::Ready(t))),
        }
    }

    /// Acquire a shared item (In the same thread) and invoke the function fut_func over it.
    /// fut_func must return an IntoFuture that resolves to a tuple of the form (t, output), 
    /// where t is the original item, and output is custom output.
    /// 
    /// This function returns a future that resolves to the value given at output.
    pub fn acquire<F,B,E,G,O>(&self, fut_func: F) -> AcquireFuture<T,F,G>
    where
        F: FnOnce(T) -> B,
        G: Future<Item=(T,O), Error=E>,
        B: IntoFuture<Item=G::Item, Error=G::Error, Future=G>,
    {
        let ref mut b_state = *self.async_mutex_state.borrow_mut();
        match mem::replace(b_state, AsyncMutexState::Empty) {
            AsyncMutexState::Empty => unreachable!(),
            AsyncMutexState::Ready(t) => {
                *b_state = AsyncMutexState::Busy(VecDeque::new());
                AcquireFuture {
                    async_mutex_state: Rc::clone(&self.async_mutex_state),
                    acquire_future_state: 
                        AcquireFutureState::WaitFunc(fut_func(t).into_future()),
                }
            },
            AsyncMutexState::Busy(mut pending) => {
                let (sender, receiver) = oneshot::channel::<T>();
                pending.push_back(sender);
                *b_state = AsyncMutexState::Busy(pending);

                AcquireFuture {
                    async_mutex_state: Rc::clone(&self.async_mutex_state),
                    acquire_future_state:
                        AcquireFutureState::WaitItem((receiver, fut_func)),
                }
            }
        }
    }
}


#[cfg(test)]
mod tests {
    extern crate tokio_core;

    use super::*;
    use self::tokio_core::reactor::Core;

    struct MyStruct {
        num: usize,
    }

    #[test]
    fn test_mutex_basic() {
        let mut core = Core::new().unwrap();
        let async_mutex = AsyncMutex::new(MyStruct { num: 0 });
        let fut1 = async_mutex.acquire(|mut my_struct| -> Result<_,()> {
            my_struct.num += 1;
            Ok((my_struct, 5))
        });

        assert_eq!(core.run(fut1).unwrap(),5);
    }

    #[test]
    fn test_mutex_multiple_acquires() {
        let mut core = Core::new().unwrap();
        let handle = core.handle();

        let async_mutex = AsyncMutex::new(MyStruct { num: 0 });

        let fut1 = async_mutex.acquire(|mut my_struct| -> Result<_,()> {
            my_struct.num += 1;
            Ok((my_struct, ()))
        });

        let fut2 = async_mutex.acquire(|mut my_struct| -> Result<_,()> {
            my_struct.num += 1;
            Ok((my_struct, ()))
        });

        let fut3 = async_mutex.acquire(|mut my_struct| -> Result<_,()> {
            my_struct.num += 1;
            Ok((my_struct, ()))
        });

        handle.spawn(fut2.map_err(|_| ()));
        handle.spawn(fut1.map_err(|_| ()));
        handle.spawn(fut3.map_err(|_| ()));

        let fut4 = async_mutex.acquire(|mut my_struct| -> Result<_,()> {
            my_struct.num += 1;
            let num = my_struct.num;
            Ok((my_struct, num))
        });

        assert_eq!(core.run(fut4).unwrap(), 4);

    }
}
