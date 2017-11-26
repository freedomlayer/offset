extern crate futures;

use std::mem;
use std::collections::VecDeque;
use std::cell::RefCell;

use self::futures::sync::oneshot;
use self::futures::future::IntoFuture;
use self::futures::{Future, Poll, Async};


enum AsyncMutexState<T> {
    Ready(T),
    Busy(VecDeque<oneshot::Sender<T>>),
    Empty,
}

struct AsyncMutex<T> {
    state: RefCell<AsyncMutexState<T>>,
}


enum AcquireFutureState<T,F,G> {
    WaitItem((oneshot::Receiver<T>, F)),
    WaitFunc(G),
    Empty,
}

struct AcquireFuture<'a,T:'a,F,G> {
    state_ref: &'a RefCell<AsyncMutexState<T>>,
    acquire_future_state: AcquireFutureState<T,F,G>,
}

#[derive(Debug)]
enum AcquireMutexError<E> {
    ReceiverCanceled,
    SendFailed,
    FuncError(E),
}

impl<'a,T,F,B,G,E,O> Future for AcquireFuture<'a,T,F,G> 
where
    F: FnOnce(T) -> B,
    G: Future<Item=(T,O), Error=E>,
    B: IntoFuture<Item=(T,O), Error=E, Future=G>
{
    type Item = O;
    type Error = AcquireMutexError<E>;

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
                        Ok(Async::NotReady) => return Ok(Async::NotReady),
                        Err(oneshot::Canceled) => return Err(AcquireMutexError::ReceiverCanceled),
                    }
                },
                AcquireFutureState::WaitFunc(mut fut_result) => {
                    match fut_result.poll() {
                        Ok(Async::NotReady) => return Ok(Async::NotReady),
                        Ok(Async::Ready((t,output))) => {
                            // We need to put the item back, and notify the next waiter on the
                            // queue that it is ready
                            let ref mut b_state_ref = *self.state_ref.borrow_mut();
                            match mem::replace(b_state_ref, 
                                               AsyncMutexState::Empty) {

                                AsyncMutexState::Empty => unreachable!(),
                                AsyncMutexState::Ready(_) => unreachable!(),
                                AsyncMutexState::Busy(mut pending) => {
                                    if let Some(sender) = pending.pop_front() {
                                        match sender.send(t) {
                                            Ok(()) => *b_state_ref = AsyncMutexState::Busy(pending),
                                            Err(t) => return Err(AcquireMutexError::SendFailed),
                                        };
                                    
                                    } else {
                                        *b_state_ref = AsyncMutexState::Ready(t);
                                    }

                                }
                            }
                            return Ok(Async::Ready(output));
                        },
                        Err(e) => return Err(AcquireMutexError::FuncError(e)),
                    }
                },
            }
        }
    }
}


impl<T> AsyncMutex<T> {
    pub fn new(t: T) -> Self {
        AsyncMutex {
            state: RefCell::new(AsyncMutexState::Ready(t)),
        }
    }

    pub fn acquire<'a,'b:'a,F,B,E,G,O>(&'b self, fut_func: F) -> AcquireFuture<'a,T,F,G>
    where
        F: FnOnce(T) -> B,
        G: Future<Item=(T,O), Error=E>,
        B: IntoFuture<Item=G::Item, Error=G::Error, Future=G>,
    {
        let ref mut b_state = *self.state.borrow_mut();
        match mem::replace(b_state, AsyncMutexState::Empty) {
            AsyncMutexState::Empty => unreachable!(),
            AsyncMutexState::Ready(t) => {
                *b_state = AsyncMutexState::Busy(VecDeque::new());
                AcquireFuture {
                    state_ref: &self.state,
                    acquire_future_state: 
                        AcquireFutureState::WaitFunc(fut_func(t).into_future()),
                }
            },
            AsyncMutexState::Busy(mut pending) => {
                let (sender, receiver) = oneshot::channel::<T>();
                pending.push_back(sender);
                *b_state = AsyncMutexState::Busy(pending);

                AcquireFuture {
                    state_ref: &self.state,
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

        handle.spawn(fut1.map_err(|_| ()));

        let fut4 = async_mutex.acquire(|mut my_struct| -> Result<_,()> {
            my_struct.num += 1;
            let num = my_struct.num;
            Ok((my_struct, num))
        });

        assert_eq!(core.run(fut4).unwrap(), 4);
    }
}
