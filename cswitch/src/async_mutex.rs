#![deny(warnings)]

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
    broken: RefCell<bool>,
}

#[derive(Debug)]
pub struct AsyncMutex<T> {
    inner: Rc<Inner<T>>,
}

#[derive(Debug)]
pub enum AcquireError {
    SenderCanceled,
    ResourceBroken,
}

#[derive(Debug)]
pub enum AsyncMutexError<E> {
    Acquire(AcquireError),
    Function(E),
}

impl<E> From<E> for AsyncMutexError<E> {
    fn from(e: E) -> AsyncMutexError<E> {
        AsyncMutexError::Function(e)
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
pub struct AcquireFuture<T, F, G>
{
    inner: Rc<Inner<T>>,
    state: AcquireFutureState<T, F, G>,
}

impl<T> AsyncMutex<T> {
    /// Create a new **single threading** shared mutex resource.
    pub fn new(t: T) -> AsyncMutex<T> {
        let inner = Rc::new(Inner {
            waiters: MsQueue::new(),
            bucket: RefCell::new(Some(t)),
            broken: RefCell::new(false),
        });

        AsyncMutex { inner }
    }

    /// Acquire a shared resource (in the same thread) and invoke the function `f` over it.
    ///
    /// The `f` MUST return an `IntoFuture` that resolves to a tuple of the form (t, output),
    /// where `t` is the original `resource`, and `output` is custom output.
    ///
    /// This function returns a future that resolves to the value given at output.
    pub fn acquire<F, B, E, G, O>(&self, f: F) -> AcquireFuture<T, F, G>
        where
            F: FnOnce(T) -> B,
            G: Future<Item=(T, O), Error=E>,
            B: IntoFuture<Item=G::Item, Error=G::Error, Future=G>,
    {
        match self.inner.bucket.replace(None) {
            None => {
                // If the resource is `None`, there will be two cases:
                //
                // 1. The resource is being used, **and there are NO other waiters**.
                // 2. The resource is being used, **and there are other waiters**.
                let (sender, receiver) = oneshot::channel::<T>();
                self.inner.waiters.push(sender);

                AcquireFuture {
                    inner: Rc::clone(&self.inner),
                    state: AcquireFutureState::WaitResource((receiver, f)),
                }
            }
            Some(t) => {
                assert!(self.inner.waiters.is_empty());

                AcquireFuture::<_, F, _> {
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
                    if *self.inner.broken.borrow() {
                        while let Some(waiter) = self.inner.waiters.try_pop() {
                            drop(waiter);
                        }
                        return Err(AsyncMutexError::Acquire(AcquireError::ResourceBroken));
                    }
                    match receiver.poll() {
                        Ok(Async::Ready(t)) => {
                            trace!("AcquireFutureState::WaitResource -- Ready");

                            self.state = AcquireFutureState::WaitFunction(f(t).into_future());
                        }
                        Ok(Async::NotReady) => {
                            trace!("AcquireFutureState::WaitResource -- NotReady");

                            self.state = AcquireFutureState::WaitResource((receiver, f));
                            return Ok(Async::NotReady);
                        }
                        Err(oneshot::Canceled) => {
                            return Err(AsyncMutexError::Acquire(AcquireError::SenderCanceled));
                        }
                    }
                }
                AcquireFutureState::WaitFunction(mut f) => {
                    match f.poll() {
                        Ok(Async::NotReady) => {
                            trace!("AcquireFutureState::WaitFunction -- NotReady");

                            self.state = AcquireFutureState::WaitFunction(f);
                            return Ok(Async::NotReady);
                        }
                        Ok(Async::Ready((resource, output))) => {
                            trace!("AcquireFutureState::WaitFunction -- Ready");

                            let mut bucket = Some(resource);

                            // There are some waiters, we wakeup the the first _alive_ waiter by
                            // sending the `resource` to him.
                            if !self.inner.waiters.is_empty() {
                                while let Some(waiter) = self.inner.waiters.try_pop() {
                                    let resource = bucket.take()
                                        .expect("Attempted to take resource after it gone");

                                    if let Err(resource) = waiter.send(resource) {
                                        bucket = Some(resource);
                                        continue;
                                    } else {
                                        break;
                                    }
                                }
                            }

                            self.inner.bucket.replace(bucket);
                            return Ok(Async::Ready(output));
                        }
                        Err(e) => {
                            debug_assert!(self.inner.bucket.borrow().is_none());

                            self.inner.broken.replace(true);

                            while let Some(waiter) = self.inner.waiters.try_pop() {
                                drop(waiter);
                            }

                            return Err(AsyncMutexError::Function(e));
                        }
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

    struct NumCell {
        num: usize,
    }

    #[test]
    fn simple() {
        let mut core = Core::new().unwrap();
        let handle = core.handle();

        let async_mutex = AsyncMutex::new(NumCell { num: 0 });

        let task1 = async_mutex.acquire(|mut num_cell| -> Result<_, ()> {
            num_cell.num += 1;
            Ok((num_cell, ()))
        });

        handle.spawn(task1.map_err(|_| ()));

        {
            let _task2 = async_mutex.acquire(|mut num_cell| -> Result<_, ()> {
                num_cell.num += 1;
                Ok((num_cell, ()))
            });

            let _task3 = async_mutex.acquire(|mut num_cell| -> Result<_, ()> {
                num_cell.num += 1;
                Ok((num_cell, ()))
            });
        }

        let task4 = async_mutex.acquire(|mut num_cell| -> Result<_, ()> {
            num_cell.num += 1;

            let num = num_cell.num;
            Ok((num_cell, num))
        });

        assert_eq!(core.run(task4).unwrap(), 2);
    }

    #[test]
    fn multiple() {
        const N: usize = 10000;

        let mut core = Core::new().unwrap();
        let handle = core.handle();

        let async_mutex = AsyncMutex::new(NumCell { num: 0 });

        for num in 0..N {
            let task = async_mutex.acquire(move |mut num_cell| -> Result<_, ()> {
                assert_eq!(num_cell.num, num);

                num_cell.num += 1;
                Ok((num_cell, ()))
            });

            handle.spawn(task.map_err(|_| ()));
        }

        let task = async_mutex.acquire(|mut num_cell| -> Result<_, ()> {
            num_cell.num += 1;
            let num = num_cell.num;
            Ok((num_cell, num))
        });

        assert_eq!(core.run(task).unwrap(), N + 1);
    }

    #[test]
    fn nested() {
        let mut core = Core::new().unwrap();
        let handle = core.handle();

        let async_mutex = AsyncMutex::new(NumCell { num: 0 });

        let task = async_mutex.clone().acquire(move |mut num_cell| -> Result<_, ()> {
            num_cell.num += 1;

            let nested_task = async_mutex.acquire(|mut num_cell| {
                assert_eq!(num_cell.num, 1);
                num_cell.num += 1;
                Ok((num_cell, ()))
            });
            handle.spawn(nested_task.map_err(|_: AsyncMutexError<()>| ()));

            let num = num_cell.num;
            Ok((num_cell, num))
        });

        assert_eq!(core.run(task).unwrap(), 1);
    }

    #[test]
    fn error() {
        let mut core = Core::new().unwrap();

        let async_mutex = AsyncMutex::new(NumCell { num: 0 });

        let task1 = async_mutex.acquire(|mut num_cell| -> Result<_, ()> {
            num_cell.num += 1;
            let num = num_cell.num;
            Ok((num_cell, num))
        });

        let task2 = async_mutex.acquire(|_| -> Result<(_, ()), ()> {
            Err(())
        });

        let task3 = async_mutex.acquire(|mut num_cell| -> Result<_, ()> {
            num_cell.num += 1;
            Ok((num_cell, ()))
        }).map_err(|_| ());

        assert_eq!(core.run(task1).unwrap(), 1);
        assert!(core.run(task2).is_err());
        assert!(core.run(task3).is_err());
    }
}
