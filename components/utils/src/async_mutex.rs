use std::sync::{Arc, Mutex};
use std::collections::LinkedList;

use futures::channel::oneshot;
use futures::{future, Future, FutureExt};


#[derive(Debug)]
struct Awakener {
    queue: LinkedList<oneshot::Sender<()>>,
}

impl Awakener {
    fn new() -> Awakener {
        Awakener {
            queue: LinkedList::new(),
        }
    }

    /// Try to send the resource.
    /// Return `None` if succeed.
    /// Return `Some(resource)` if failed.
    fn wakeup_next(&mut self) {
        while let Some(sender) = self.queue.pop_front() {
            if let Ok(_) = sender.send(()) {
                break;
            }
        }
    }

    /// Make a pair of `(sender, receiver)`.
    /// `sender` is pushed to `self.queue`.
    /// Return `receiver`.
    fn add_awakener(&mut self) -> oneshot::Receiver<()> {
        let (sender, receiver) = oneshot::channel();
        self.queue.push_back(sender);
        receiver
    }

    fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }
}

#[derive(Debug)]
struct Inner<T> {
    mutex_resource: Mutex<T>,
    mutex_opt_pending: Mutex<Option<Awakener>>,
}

#[derive(Debug)]
pub struct AsyncMutex<T> {
    arc_inner: Arc<Inner<T>>,
}

impl<T> AsyncMutex<T> {
    pub fn new(resource: T) -> AsyncMutex<T> {
        let inner = Inner {
            mutex_resource: Mutex::new(resource),
            mutex_opt_pending: Mutex::new(None),
        };
        AsyncMutex {
            arc_inner: Arc::new(inner),
        }
    }

    pub fn acquire_borrow<'a,F:'a,B:'a,O:'a>(&'a self, f: F) -> impl Future<Output=O> + '_
    where
        F: FnOnce(&mut T) -> B,
        B: Future<Output=O>,
    {
        future::lazy(|_| ())
            .then(move |_| self.acquire_borrow_inner(f))
    }

    fn acquire_borrow_inner<'a,F:'a,B:'a,O:'a>(&'a self, f: F) -> impl Future<Output=O>  + '_
    where
        F: FnOnce(&mut T) -> B,
        B: Future<Output=O>,
    {

        let inner = &*self.arc_inner;
        let fut_wait = {
            let mut opt_pending_guard = inner.mutex_opt_pending.lock().unwrap();

            if let Some(pending) = &mut *opt_pending_guard {
                // Register for waiting:
                pending.add_awakener()
            } else {
                *opt_pending_guard = Some(Awakener::new());
                let (sender, receiver) = oneshot::channel::<()>();
                sender.send(()).unwrap();
                receiver
            }
        };

        fut_wait
            .then(move |_| {
                let mut resource_guard = inner.mutex_resource.lock().unwrap();
                f(&mut *resource_guard)
            }).then(move |output| {
                let mut opt_pending_guard = inner.mutex_opt_pending.lock().unwrap();
                if let Some(mut pending_guard) = (&mut *opt_pending_guard).take() {
                    if pending_guard.is_empty() {
                        *opt_pending_guard = None;
                    } else {
                        pending_guard.wakeup_next();
                    }
                }
                future::ready(output)
            })

    }
}


impl<T> Clone for AsyncMutex<T> {
    fn clone(&self) -> AsyncMutex<T> {
        AsyncMutex {
            arc_inner: Arc::clone(&self.arc_inner),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::{FutureExt, SinkExt, StreamExt};
    use futures::task::{SpawnExt};
    use futures::executor::ThreadPool;
    use futures::channel::mpsc;

    struct NumCell {
        num: usize,
    }

    #[test]
    fn borrow_simple() {
        let mut thread_pool = ThreadPool::new().unwrap();

        let async_mutex = AsyncMutex::new(NumCell { num: 0 });

        let task1 = async_mutex.acquire_borrow(|num_cell| {
            num_cell.num += 1;
            future::ready(())
        });

        thread_pool.run(task1);

        {
            let _ = async_mutex.acquire_borrow(|num_cell| {
                num_cell.num += 1;
                future::ready(())
            });

            let _ = async_mutex.acquire_borrow(|num_cell| {
                num_cell.num += 1;
                future::ready(())
            });
        }

        let task2 = async_mutex.acquire_borrow(|num_cell| {
            num_cell.num += 1;
            let num = num_cell.num;
            future::ready(num)
        });

        assert_eq!(thread_pool.run(task2), 2);
    }

    #[test]
    fn borrow_multiple() {
        const N: usize = 1_000;
        let async_mutex = AsyncMutex::new(NumCell { num: 0 });

        let mut thread_pool = ThreadPool::new().unwrap();
        let (sender, mut receiver) = mpsc::channel::<()>(0);

        for _ in 0..N {
            let c_async_mutex = async_mutex.clone();
            let mut c_sender = sender.clone();
            let task = async move {
                await!(c_async_mutex.acquire_borrow(move |num_cell| {
                    num_cell.num += 1;
                    future::ready(())
                }));
                await!(c_sender.send(())).unwrap();
            };
            thread_pool.spawn(task.map(|_| ())).unwrap();
        }

        let task = async_mutex.acquire_borrow(|num_cell| {
            num_cell.num += 1;
            let num = num_cell.num;
            future::ready(num)
        });

        thread_pool.run(async move {
            for _ in 0 .. N {
                await!(receiver.next()).unwrap();
            }
        });

        assert_eq!(thread_pool.run(task), N + 1);
    }

    #[test]
    fn borrow_nested() {
        let mut thread_pool = ThreadPool::new().unwrap();

        let async_mutex = AsyncMutex::new(NumCell { num: 0 });

        let task = async move {
            let c_async_mutex = async_mutex.clone();
            await!(async_mutex.acquire_borrow(move |num_cell| {
                num_cell.num += 1;

                let mut _nested_task = c_async_mutex.acquire_borrow(|num_cell| {
                    assert_eq!(num_cell.num, 1);
                    num_cell.num += 1;
                    future::ready(())
                });

                let num = num_cell.num;
                future::ready(num)
            }))
        };

        assert_eq!(thread_pool.run(task), 1);
    }

}
