#![allow(unused)]

use std::collections::{HashMap, HashSet};
use std::mem;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

use futures::future::{self, FutureObj};
use futures::task::{waker, ArcWake, Context, Poll, Spawn, SpawnError, Waker};
use futures::Future;

// use crate::caller_info::{get_caller_info, CallerInfo};

struct TestExecutorInner {
    /// The id that will be given to the next registered future:
    next_id: usize,
    /// Futures in progress:
    futures: HashMap<usize, Pin<Box<dyn Future<Output = ()> + Send>>>,
    /// id of current future being polled:
    opt_polled_id: Option<usize>,
    /// Next set of futures we need to wake:
    new_wakes: Vec<usize>,
    /// A future that waits for state of no progress
    /// of all other futures
    opt_waiter_id: Option<usize>,
}

impl TestExecutorInner {
    fn new() -> Self {
        TestExecutorInner {
            next_id: 0,
            futures: HashMap::new(),
            opt_polled_id: None,
            new_wakes: Vec::new(),
            opt_waiter_id: None,
        }
    }

    fn add_wake(&mut self, future_id: usize) {
        self.new_wakes
            .retain(|cur_future_id| cur_future_id != &future_id);
        self.new_wakes.push(future_id);
    }
}

struct TestWaker {
    future_id: usize,
    arc_mutex_inner: Arc<Mutex<TestExecutorInner>>,
}

impl TestWaker {
    fn new(future_id: usize, arc_mutex_inner: Arc<Mutex<TestExecutorInner>>) -> Self {
        TestWaker {
            future_id,
            arc_mutex_inner,
        }
    }
}

impl ArcWake for TestWaker {
    fn wake_by_ref(arc_self: &Arc<Self>) {
        let mut inner = arc_self.arc_mutex_inner.lock().unwrap();
        // Avoid duplicates:
        inner.add_wake(arc_self.future_id);
    }
}

#[derive(Clone)]
pub struct TestExecutor {
    arc_mutex_inner: Arc<Mutex<TestExecutorInner>>,
}

impl TestExecutor {
    pub fn new() -> Self {
        TestExecutor {
            arc_mutex_inner: Arc::new(Mutex::new(TestExecutorInner::new())),
        }
    }

    /// Prepare a waker for a future with `future_id`
    fn create_waker(&self, future_id: usize) -> Waker {
        let test_waker = TestWaker::new(future_id, self.arc_mutex_inner.clone());
        waker(Arc::new(test_waker))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunOutput<T> {
    /// Output has returned
    Output(T),
    /// No more progress is possible
    NoProgress,
}

impl<T> RunOutput<T> {
    /// Returns output if RunOutput contains an output
    pub fn output(self) -> Option<T> {
        match self {
            RunOutput::Output(t) => Some(t),
            RunOutput::NoProgress => None,
        }
    }

    /// Check if RunOutput contains an output
    pub fn is_output(&self) -> bool {
        match self {
            RunOutput::Output(_t) => true,
            RunOutput::NoProgress => false,
        }
    }
}

/// A future that resolves when no more progress
/// can be made in all other futures inside the executor.
pub struct WaitFuture {
    /// Was this future ever polled?
    was_polled: bool,
}

impl WaitFuture {
    fn new() -> Self {
        WaitFuture { was_polled: false }
    }
}

/// WaitFuture is a future that resolves to ()
/// the second time it is polled.
impl Future for WaitFuture {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, context: &mut Context) -> Poll<Self::Output> {
        if self.as_mut().was_polled {
            Poll::Ready(())
        } else {
            self.was_polled = true;
            Poll::Pending
        }
    }
}

impl TestExecutor {
    /// Perform one iteration of polling all futures that needs to be awaken.
    /// If the local future is resolved, we return its output.
    fn poll_future(&self, future_id: usize, context: &mut Context) {
        let mut future = {
            let mut inner = self.arc_mutex_inner.lock().unwrap();
            if let Some(future) = inner.futures.remove(&future_id) {
                future
            } else {
                warn!("Nonexistent future was polled");
                return;
            }
        };

        // Mark current future being polled:
        {
            let mut inner = self.arc_mutex_inner.lock().unwrap();
            inner.opt_polled_id = Some(future_id);
        }
        let poll_res = future.as_mut().poll(context);
        // Clear current future being polled:
        {
            let mut inner = self.arc_mutex_inner.lock().unwrap();
            inner.opt_polled_id.take();
        }

        match poll_res {
            Poll::Pending => {
                // Put the future back:
                let mut inner = self.arc_mutex_inner.lock().unwrap();
                inner.futures.insert(future_id, future);
            }
            Poll::Ready(()) => {}
        };
    }

    /// Perform one iteration of polling all futures that needs to be awaken.
    /// If the local future is resolved, we return its output.
    fn wake_iter_local_future<F>(
        &self,
        local_future_id: usize,
        local_future: &mut Pin<Box<F>>,
    ) -> Option<F::Output>
    where
        F: Future,
    {
        let new_wakes = {
            let mut inner = self.arc_mutex_inner.lock().unwrap();
            mem::replace(&mut inner.new_wakes, Vec::new())
        };

        // Output from local future (If resolved):
        let mut opt_local_output = None;

        for future_id in new_wakes {
            {
                // If this is the waiting future, we don't poll it:
                let mut inner = self.arc_mutex_inner.lock().unwrap();
                if Some(future_id) == inner.opt_waiter_id {
                    continue;
                }
            }

            let waker = self.create_waker(future_id);
            let mut context = Context::from_waker(&waker);
            if future_id == local_future_id {
                // Woken future is our local future:

                // Mark current future being polled:
                {
                    let mut inner = self.arc_mutex_inner.lock().unwrap();
                    inner.opt_polled_id = Some(future_id);
                }
                let poll_res = local_future.as_mut().poll(&mut context);
                // Clear current future being polled:
                {
                    let mut inner = self.arc_mutex_inner.lock().unwrap();
                    inner.opt_polled_id.take();
                }

                if let Poll::Ready(t) = poll_res {
                    opt_local_output = Some(t);
                }
                continue;
            }

            self.poll_future(future_id, &mut context);
        }
        opt_local_output
    }

    pub fn run<F: Future>(&self, local_future: F) -> RunOutput<F::Output> {
        // Register local_future:
        let local_future_id = {
            let mut inner = self.arc_mutex_inner.lock().unwrap();
            let local_future_id = inner.next_id;
            inner.next_id = inner.next_id.checked_add(1).unwrap();
            // Insert our new future's id to the new_wakes list:
            inner.add_wake(local_future_id);
            local_future_id
        };

        // Pin the future.
        // TODO: How can we pin the future to the stack using a safe interface?
        let mut boxed_local_future = Box::pin(local_future);

        loop {
            if let Some(local_output) =
                self.wake_iter_local_future(local_future_id, &mut boxed_local_future)
            {
                return RunOutput::Output(local_output);
            }
            {
                let mut inner = self.arc_mutex_inner.lock().unwrap();
                if inner.new_wakes.is_empty() {
                    if let Some(waiter_id) = inner.opt_waiter_id.take() {
                        inner.add_wake(waiter_id);
                    } else {
                        break;
                    }
                }
            }
        }

        RunOutput::NoProgress
    }

    pub fn wait(&self) -> WaitFuture {
        let mut inner = self.arc_mutex_inner.lock().unwrap();
        if inner.opt_waiter_id.is_some() {
            panic!("Another future is already wait()-ing");
        }

        match inner.opt_polled_id {
            Some(polled_id) => inner.opt_waiter_id = Some(polled_id),
            None => panic!("wait() called from outside a future"),
        };

        WaitFuture::new()
    }

    /// Perform one iteration of polling all futures that needs to be awaken.
    fn wake_iter(&mut self) {
        let new_wakes = {
            let mut inner = self.arc_mutex_inner.lock().unwrap();
            mem::replace(&mut inner.new_wakes, Vec::new())
        };

        for future_id in new_wakes {
            {
                // If this is the waiting future, we don't poll it:
                let mut inner = self.arc_mutex_inner.lock().unwrap();
                if Some(future_id) == inner.opt_waiter_id {
                    continue;
                }
            }

            let waker = self.create_waker(future_id);
            let mut context = Context::from_waker(&waker);
            self.poll_future(future_id, &mut context);
        }
    }

    /// Run until no more progress is possible
    pub fn run_until_no_progress(&mut self) {
        loop {
            self.wake_iter();

            let mut inner = self.arc_mutex_inner.lock().unwrap();
            if inner.new_wakes.is_empty() {
                if let Some(waiter_id) = inner.opt_waiter_id.take() {
                    inner.add_wake(waiter_id);
                } else {
                    break;
                }
            }
        }
    }
}

impl Spawn for TestExecutor {
    /// Spawn a new future
    fn spawn_obj(&self, future: FutureObj<'static, ()>) -> Result<(), SpawnError> {
        let mut inner = self.arc_mutex_inner.lock().unwrap();

        // Obtain an id for the future:
        let future_id = inner.next_id;
        inner.next_id = inner.next_id.checked_add(1).unwrap();

        // Insert our new future's id to the new_wakes list:
        inner.add_wake(future_id);

        // Insert the future into the futures map:
        inner.futures.insert(future_id, Box::pin(future));

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::channel::{mpsc, oneshot};
    use futures::task::SpawnExt;
    use futures::{SinkExt, StreamExt};

    #[test]
    fn test_lazy_future() {
        let mut test_executor = TestExecutor::new();

        let res = test_executor.run(future::lazy(|_| ()));
        assert_eq!(res.output().unwrap(), ());
    }

    #[test]
    fn test_spawn() {
        let mut test_executor = TestExecutor::new();

        let (sender, receiver) = oneshot::channel();

        let res = test_executor.spawn(async move {
            sender.send(());
        });

        let res = test_executor.run(receiver);
        assert_eq!(res.output().unwrap().unwrap(), ());
    }

    #[test]
    fn test_one_future() {
        let mut test_executor = TestExecutor::new();

        let (mut init_sender, mut init_receiver) = oneshot::channel();
        let arc_mutex_res = Arc::new(Mutex::new(false));

        let c_arc_mutex_res = arc_mutex_res.clone();
        test_executor
            .spawn(async move {
                init_receiver.await.unwrap();

                let (mut a_sender, mut a_receiver) = mpsc::channel::<u32>(1);
                a_sender.send(0).await.unwrap();
                assert_eq!(a_receiver.next().await.unwrap(), 0);

                let mut res_guard = c_arc_mutex_res.lock().unwrap();
                *res_guard = true;
            })
            .unwrap();

        let mut c_test_executor = test_executor.clone();
        let res = test_executor.run(async move {
            init_sender.send(()).unwrap();
            c_test_executor.wait().await;
            0x1337
        });

        assert_eq!(res.output().unwrap(), 0x1337);

        let res_guard = arc_mutex_res.lock().unwrap();
        assert!(*res_guard);
    }

    // Based on:
    // - https://rust-lang-nursery.github.io/futures-api-docs/0.3.0-alpha.13/src/futures_test/future/pending_once.rs.html#14-17
    // - https://github.com/rust-lang-nursery/futures-rs/issues/869
    pub struct Yield(usize);

    impl Yield {
        pub fn new(num_yields: usize) -> Self {
            Yield(num_yields)
        }
    }

    impl Future for Yield {
        type Output = ();
        fn poll(mut self: Pin<&mut Self>, context: &mut Context) -> Poll<Self::Output> {
            let count = &mut self.as_mut().0;
            *count = count.saturating_sub(1);
            if *count == 0 {
                Poll::Ready(())
            } else {
                context.waker().clone().wake();
                Poll::Pending
            }
        }
    }

    #[test]
    fn test_yield() {
        let mut test_executor = TestExecutor::new();

        for _ in 0..8 {
            test_executor.spawn(Yield::new(0x10)).unwrap();
        }

        let mut c_test_executor = test_executor.clone();
        test_executor.run(async move {
            c_test_executor.wait().await;
        });
    }

    #[test]
    fn test_channel_full() {
        let mut test_executor = TestExecutor::new();

        let arc_mutex_res = Arc::new(Mutex::new(0usize));
        let c_arc_mutex_res = Arc::clone(&arc_mutex_res);
        test_executor
            .spawn(async move {
                // Channel has limited capacity:
                let (mut sender, _receiver) = mpsc::channel::<u32>(8);

                // We keep sending into the channel.
                // At some point this loop should be stuck, because the channel is full.
                loop {
                    sender.send(0).await.unwrap();
                    let mut res_guard = c_arc_mutex_res.lock().unwrap();
                    *res_guard = res_guard.checked_add(1).unwrap();
                }
            })
            .unwrap();

        test_executor.run_until_no_progress();
        let res_guard = arc_mutex_res.lock().unwrap();
        assert_eq!(*res_guard, 8);
    }

    #[test]
    fn test_spawn_with_handle() {
        let mut test_executor = TestExecutor::new();

        let handle = test_executor
            .spawn_with_handle(future::ready(0x1337u32))
            .unwrap();

        assert_eq!(test_executor.run(handle).output().unwrap(), 0x1337u32);
        test_executor.run_until_no_progress();
    }
}
