use std::mem;
use std::rc::Rc;
use std::cell::RefCell;

use futures::sync::oneshot;
use futures::{Future, Poll, Async, IntoFuture};

use crossbeam::sync::MsQueue;

#[derive(Debug)]
pub struct Inner<T> {
    waiters: MsQueue<oneshot::Sender<T>>,
    bucket: RefCell<Option<T>>,
    health: RefCell<bool>,
}

#[derive(Debug)]
pub struct AsyncMutex<T> {
    inner: Rc<Inner<T>>,
}

#[derive(Debug)]
pub enum IoError {
    ReceiverCanceled,
    SendFailed,
    ResourceBroken,
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
#[must_use = "futures do nothing unless polled"]
pub struct AcquireFuture<T, F, G> {
    inner: Rc<Inner<T>>,
    state: AcquireFutureState<T, F, G>,
}

impl<T> AsyncMutex<T> {
    /// Create a new **single threading** shared mutex resource.
    pub fn new(t: T) -> AsyncMutex<T> {
        let inner = Rc::new(Inner {
            waiters: MsQueue::new(),
            bucket: RefCell::new(Some(t)),
            health: RefCell::new(true),
        });

        AsyncMutex { inner }
    }

    /// Acquire a shared resource (in the same thread) and invoke the function `f` over it.
    ///
    /// The `f` MUST return an `IntoFuture` that resolves to a tuple of the form (t, output),
    /// where `t` is the original `resource`, and `output` is custom output.
    ///
    /// This function returns a future that resolves to the value given at output.
    pub fn acquire<F, B, E, G, O>(&self, f: F) -> Result<AcquireFuture<T, F, G>, AsyncMutexError<E>>
        where
            F: FnOnce(T) -> B,
            G: Future<Item=(T, O), Error=E>,
            B: IntoFuture<Item=G::Item, Error=G::Error, Future=G>,
    {
        if !(*self.inner.health.borrow()) {
            Err(AsyncMutexError::IoError(IoError::ResourceBroken))
        } else {
            match self.inner.bucket.replace(None) {
                None => {
                    // If the resource is `None`, there will be two cases:
                    //
                    // 1. The resource is being used, **and there are NO other waiters**.
                    // 2. The resource is being used, **and there are other waiters **.
                    let (sender, receiver) = oneshot::channel::<T>();
                    self.inner.waiters.push(sender);

                    Ok(AcquireFuture {
                        inner: Rc::clone(&self.inner),
                        state: AcquireFutureState::WaitResource((receiver, f)),
                    })
                },
                Some(t) => {
                    assert!(self.inner.waiters.is_empty());

                    Ok(AcquireFuture {
                        inner: Rc::clone(&self.inner),
                        state: AcquireFutureState::WaitFunction(f(t).into_future()),
                    })
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

                            let mut bucket = Some(resource);

                            // There are some waiters, we send the `resource`
                            // to the first _alive_ waiter.
                            if !self.inner.waiters.is_empty() {
                                while let Some(waiter) = self.inner.waiters.try_pop() {
                                    assert!(bucket.is_some(), "unexpected status");

                                    let resource = mem::replace(&mut bucket, None).unwrap();
                                    if let Err(resource) = waiter.send(resource) {
                                        bucket = Some(resource);
                                        continue;
                                    }

                                    break;
                                }
                            }

                            self.inner.bucket.replace(bucket);
                            return Ok(Async::Ready(output));
                        },
                        Err(e) => {
                            debug_assert!(self.inner.bucket.borrow().is_none());

                            self.inner.health.replace(false);

                            while let Some(waiter) = self.inner.waiters.try_pop() {
                                drop(waiter);
                            }

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
        }).unwrap();

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
        }).unwrap();

        let fut2 = async_mutex.acquire(|mut my_struct| -> Result<_, ()> {
            my_struct.num += 1;
            Ok((my_struct, ()))
        }).unwrap();

        let fut3 = async_mutex.acquire(|mut my_struct| -> Result<_, ()> {
            my_struct.num += 1;
            Ok((my_struct, ()))
        }).unwrap();

        handle.spawn(fut2.map_err(|_| ()));
        handle.spawn(fut1.map_err(|_| ()));
        handle.spawn(fut3.map_err(|_| ()));

        let fut4 = async_mutex.acquire(|mut my_struct| -> Result<_, ()> {
            my_struct.num += 1;
            let num = my_struct.num;
            Ok((my_struct, num))
        }).unwrap();

        assert_eq!(core.run(fut4).unwrap(), 4);
    }

    #[test]
    fn test_async_mutex_drop() {
        let mut core = Core::new().unwrap();
        let handle = core.handle();

        let async_mutex = AsyncMutex::new(MyStruct { num: 0 });

        let fut1 = async_mutex.acquire(|mut my_struct| -> Result<_, ()> {
            my_struct.num += 1;
            Ok((my_struct, ()))
        }).unwrap();

        handle.spawn(fut1.map_err(|_| ()));

        {
            let _fut2 = async_mutex.acquire(|mut my_struct| -> Result<_, ()> {
                my_struct.num += 1;
                Ok((my_struct, ()))
            });

            let _fut3 = async_mutex.acquire(|mut my_struct| -> Result<_, ()> {
                my_struct.num += 1;
                Ok((my_struct, ()))
            });
        }

        // TODO: Fix the bug and uncomment
        // let _fut4 = async_mutex.acquire(|mut my_struct| -> Result<_, ()> {
        //     my_struct.num += 1;
        //
        //     let num = my_struct.num;
        //     Ok((my_struct, num))
        // });

        let fut5 = async_mutex.acquire(|mut my_struct| -> Result<_, ()> {
            my_struct.num += 1;

            let num = my_struct.num;
            Ok((my_struct, num))
        }).unwrap();

        assert_eq!(core.run(fut5).unwrap(), 2);
    }

    #[test]
    fn test_async_mutex_clone() {
        let mut core = Core::new().unwrap();
        let handle = core.handle();

        let async_mutex = AsyncMutex::new(MyStruct { num: 0 });

        let fut1 = async_mutex.acquire(|mut my_struct| -> Result<_, ()> {
            my_struct.num += 1;
            Ok((my_struct, ()))
        }).unwrap();

        // Note that clone here:
        let fut2 = async_mutex.clone().acquire(|mut my_struct| -> Result<_, ()> {
            my_struct.num += 1;
            Ok((my_struct, ()))
        }).unwrap();

        let fut3 = async_mutex.acquire(|mut my_struct| -> Result<_, ()> {
            my_struct.num += 1;
            Ok((my_struct, ()))
        }).unwrap();

        handle.spawn(fut2.map_err(|_| ()));
        handle.spawn(fut1.map_err(|_| ()));
        handle.spawn(fut3.map_err(|_| ()));

        let fut4 = async_mutex.acquire(|mut my_struct| -> Result<_, ()> {
            my_struct.num += 1;
            let num = my_struct.num;
            Ok((my_struct, num))
        }).unwrap();

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
            }).unwrap();
            handle.spawn(fut2.map_err(|_: AsyncMutexError<()>| ()));

            let num = my_struct.num;
            Ok((my_struct, num))
        }).unwrap();
        assert_eq!(core.run(fut1).unwrap(), 1);
    }

    #[test]
    fn test_async_mutex_error() {
        let mut core = Core::new().unwrap();
        let handle = core.handle();

        let async_mutex = AsyncMutex::new(MyStruct { num: 0 });
        let async_mutex_inner = async_mutex.clone();

        let fut1 = async_mutex.acquire(|mut my_struct| -> Result<_, ()> {
            my_struct.num += 1;

            let num = my_struct.num;
            Ok((my_struct, num))
        }).unwrap();

        let fut2 = async_mutex.acquire(|mut my_struct| -> Result<(_, ()), ()> {
            Err(())
        }).unwrap();

        let fut3 = async_mutex.acquire(|mut my_struct| -> Result<_, ()> {
            my_struct.num += 1;
            Ok((my_struct, ()))
        }).into_future().map_err(|_| ()).and_then(|acquire_fut| {
            acquire_fut.map_err(|_| ())
        });

        assert_eq!(core.run(fut1).unwrap(), 1);
        assert!(core.run(fut2).is_err());
        assert!(core.run(fut3).is_err());
    }
}


// TODO: Add some more tests.
