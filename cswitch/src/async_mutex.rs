use std::mem;
use std::rc::Rc;
use std::cell::RefCell;

use futures::sync::oneshot;
use futures::{Future, Poll, Async, IntoFuture};

use crossbeam::sync::MsQueue;

#[derive(Debug)]
pub struct Inner<T> {
    waiters: MsQueue<oneshot::Sender<T>>,
    resource: RefCell<Option<T>>,
}

#[derive(Debug)]
pub struct AsyncMutex<T> {
    inner: Rc<Inner<T>>,
}

#[derive(Debug)]
pub enum IoError {
    ReceiverCanceled,
    SendFailed,
}

#[derive(Debug)]
pub enum AsyncMutexError<E> {
    IoError(IoError),
    FuncError(E),
}

impl<E> From<E> for AsyncMutexError<E> {
    fn from(e: E) -> AsyncMutexError<E> {
        AsyncMutexError::FuncError(e)
    }
}

#[derive(Debug)]
enum AcquireFutureState<T, F, G> {
    WaitResource((oneshot::Receiver<T>, F)),
    WaitFunction(G),
    Empty,
}

#[derive(Debug)]
pub struct AcquireFuture<T, F, G> {
    inner: Rc<Inner<T>>,
    state: AcquireFutureState<T, F, G>,
}

impl<T> AsyncMutex<T> {
    pub fn new(t: T) -> AsyncMutex<T> {
        let inner = Rc::new(Inner {
            waiters: MsQueue::new(),
            resource: RefCell::new(Some(t)),
        });

        AsyncMutex { inner }
    }

    pub fn acquire<F, B, E, G, O>(&self, f: F) -> AcquireFuture<T, F, G>
        where
            F: FnOnce(T) -> B,
            G: Future<Item=(T, O), Error=E>,
            B: IntoFuture<Item=G::Item, Error=G::Error, Future=G>,
    {
        match self.inner.resource.replace(None) {
            None => {
                let (sender, receiver) = oneshot::channel::<T>();
                self.inner.waiters.push(sender);

                AcquireFuture {
                    inner: Rc::clone(&self.inner),
                    state: AcquireFutureState::WaitResource((receiver, f)),
                }
            },
            Some(t) => {
                assert!(self.inner.waiters.is_empty());
                AcquireFuture {
                    inner: Rc::clone(&self.inner),
                    state: AcquireFutureState::WaitFunction(f(t).into_future()),
                }
            }
        }
    }
}

impl<T, F, B, G, E, O> Future for AcquireFuture<T, F, G>
    where
        F: FnOnce(T) -> B,
        G: Future<Item=(T, O), Error=E>,
        B: IntoFuture<Item=G::Item, Error=G::Error, Future=G>
{
    type Item = O;
    type Error = AsyncMutexError<E>;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        loop {
            match mem::replace(&mut self.state, AcquireFutureState::Empty) {
                AcquireFutureState::Empty => unreachable!(),
                AcquireFutureState::WaitResource((mut receiver, f)) => {
                    match receiver.poll() {
                        Ok(Async::Ready(t)) => {
                            debug!("AcquireFutureState::WaitResource -- Ready");
                            self.state = AcquireFutureState::WaitFunction(f(t).into_future());
                        },
                        Ok(Async::NotReady) => {
                            debug!("AcquireFutureState::WaitResource -- NotReady");
                            self.state = AcquireFutureState::WaitResource((receiver, f));
                            return Ok(Async::NotReady);
                        },
                        Err(oneshot::Canceled) => {
                            return Err(AsyncMutexError::IoError(IoError::ReceiverCanceled));
                        },
                    }
                }
                AcquireFutureState::WaitFunction(mut f) => {
                    match f.poll() {
                        Ok(Async::NotReady) => {
                            debug!("AcquireFutureState::WaitFunction -- NotReady");
                            self.state = AcquireFutureState::WaitFunction(f);
                            return Ok(Async::NotReady)
                        },
                        Ok(Async::Ready((resource, output))) => {
                            debug!("AcquireFutureState::WaitFunction -- Ready");
                            if let Some(waiter) = self.inner.waiters.try_pop() {
                                if let Err(resource) = waiter.send(resource) {
                                    self.inner.resource.replace(Some(resource));
                                    return Err(AsyncMutexError::IoError(IoError::SendFailed));
                                }
                            } else {
                                self.inner.resource.replace(Some(resource));
                            }
                            return Ok(Async::Ready(output));
                        },
                        Err(e) => {
                            return Err(AsyncMutexError::FuncError(e));
                        },
                    }
                }
            }
        }
    }
}

impl<T> Clone for AsyncMutex<T> {
    fn clone(&self) -> AsyncMutex<T> {
        AsyncMutex {
            inner: Rc::clone(&self.inner)
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
    fn test_async_mutex_basic() {
        let mut core = Core::new().unwrap();
        let async_mutex = AsyncMutex::new(MyStruct { num: 0 });
        let fut1 = async_mutex.acquire(|mut my_struct| -> Result<_, ()> {
            my_struct.num += 1;
            Ok((my_struct, 5))
        });

        assert_eq!(core.run(fut1).unwrap(), 5);
    }

    #[test]
    fn test_async_mutex_multiple_acquires() {
        let mut core = Core::new().unwrap();
        let handle = core.handle();

        let async_mutex = AsyncMutex::new(MyStruct { num: 0 });

        let fut1 = async_mutex.acquire(|mut my_struct| -> Result<_, ()> {
            my_struct.num += 1;
            Ok((my_struct, ()))
        });

        let fut2 = async_mutex.acquire(|mut my_struct| -> Result<_, ()> {
            my_struct.num += 1;
            Ok((my_struct, ()))
        });

        let fut3 = async_mutex.acquire(|mut my_struct| -> Result<_, ()> {
            my_struct.num += 1;
            Ok((my_struct, ()))
        });

        handle.spawn(fut2.map_err(|_| ()));
        handle.spawn(fut1.map_err(|_| ()));
        handle.spawn(fut3.map_err(|_| ()));

        let fut4 = async_mutex.acquire(|mut my_struct| -> Result<_, ()> {
            my_struct.num += 1;
            let num = my_struct.num;
            Ok((my_struct, num))
        });

        assert_eq!(core.run(fut4).unwrap(), 4);
    }

    #[test]
    fn test_async_mutex_clone() {
        let mut core = Core::new().unwrap();
        let handle = core.handle();

        let async_mutex = AsyncMutex::new(MyStruct { num: 0 });

        let fut1 = async_mutex.acquire(|mut my_struct| -> Result<_, ()> {
            my_struct.num += 1;
            Ok((my_struct, ()))
        });

        // Note that clone here:
        let fut2 = async_mutex.clone().acquire(|mut my_struct| -> Result<_, ()> {
            my_struct.num += 1;
            Ok((my_struct, ()))
        });

        let fut3 = async_mutex.acquire(|mut my_struct| -> Result<_, ()> {
            my_struct.num += 1;
            Ok((my_struct, ()))
        });

        handle.spawn(fut2.map_err(|_| ()));
        handle.spawn(fut1.map_err(|_| ()));
        handle.spawn(fut3.map_err(|_| ()));

        let fut4 = async_mutex.acquire(|mut my_struct| -> Result<_, ()> {
            my_struct.num += 1;
            let num = my_struct.num;
            Ok((my_struct, num))
        });

        assert_eq!(core.run(fut4).unwrap(), 4);
    }

    #[test]
    fn test_async_mutex_nested() {
        let mut core = Core::new().unwrap();
        let handle = core.handle();

        let async_mutex = AsyncMutex::new(MyStruct { num: 0 });
        let async_mutex_inner = async_mutex.clone();

        let fut1 = async_mutex.acquire(|mut my_struct| -> Result<_, ()> {
            my_struct.num += 1;

            let fut2 = async_mutex_inner.acquire(|mut my_struct| {
                assert_eq!(my_struct.num, 1);
                my_struct.num += 1;
                Ok((my_struct, ()))
            });
            handle.spawn(fut2.map_err(|_: AsyncMutexError<()>| ()));

            let num = my_struct.num;
            Ok((my_struct, num))
        });
        assert_eq!(core.run(fut1).unwrap(), 1);
    }
}


// TODO: Add some more tests.
