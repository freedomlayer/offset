use std::sync::Arc;
use std::sync::Mutex;
use std::collections::HashSet;
use std::pin::Pin;

use futures::task::{Spawn, SpawnError, Waker, ArcWake};
use futures::future::FutureObj;
use futures::{Poll, Future};

/// Collect tracking information about calls to poll() over futures
/// and wake() over wakers.
/// The tracking information is used to determine if no more progress is expected to happen on the
/// wrapped spawner.
struct Tracker {
    /// Tasks that should be polled again:
    pending: HashSet<usize>,
    /// Number of ongoing polls:
    ongoing_polls: usize,
    /// The id of the next spawned future:
    next_id: usize,
    /// Clients that wait for progress stop
    wakers: Vec<Waker>,
}

impl Tracker {
    pub fn new() -> Self {
        Tracker {
            pending: HashSet::new(),
            ongoing_polls: 0,
            next_id: 0,
            wakers: Vec::new(),
        }
    }

    /// Add a waker of a waiting client.
    /// A client is notified when no progress is expected to be made, and then removed.
    pub fn add_waker(&mut self, waker: Waker) {
        self.wakers.push(waker);
    }

    /// Get the next id for a spawned future.
    /// Every call to next_id() should return a unique id.
    pub fn next_id(&mut self) -> usize {
        let id = self.next_id;
        self.next_id = self.next_id.checked_add(1).unwrap();
        id
    }

    /// Insert an id of a pending spawned future (We expect that it will
    /// be rescheduled later)
    pub fn insert(&mut self, id: usize) {
        self.pending.insert(id);
    }

    /// Remove a pending spawned future
    pub fn remove(&mut self, id: &usize) {
        let res = self.pending.remove(id);
        assert!(res);
    }

    /// Is any progress possible?
    pub fn progress_done(&self) -> bool {
        self.pending.is_empty() && self.ongoing_polls == 0
    }

    /// Mark the beginning of a call to poll.
    pub fn poll_begin(&mut self) {
        self.ongoing_polls = self.ongoing_polls.checked_add(1).unwrap();
    }

    /// Mark the ending of a call to poll.
    /// If it seems like no progress is expected to happen, we notify all clients.
    pub fn poll_end(&mut self) {
        self.ongoing_polls = self.ongoing_polls.checked_sub(1).unwrap();
        if !self.progress_done() {
            return;
        }

        // No more progress can happen.
        // Notify all clients:
        while let Some(waker) = self.wakers.pop() {
            waker.wake();
        }
    }
}

/// A wrapper for a spawner that tracks all spawned futures
/// and detects whether any progress is possible.
///
/// This is useful for waiting until no more progress is possible
/// for the futures spawned through this spawner.
#[derive(Clone)]
pub struct SpawnerWait<S> {
    spawner: S,
    arc_mutex_tracker: Arc<Mutex<Tracker>>,
}

/// A future that resolves when no more progress
/// is possible
pub struct ProgressDone {
    arc_mutex_tracker: Arc<Mutex<Tracker>>,
    waker_saved: bool,
}

impl ProgressDone {
    fn new(arc_mutex_tracker: Arc<Mutex<Tracker>>) -> Self {
        ProgressDone {
            arc_mutex_tracker,
            waker_saved: false,
        }
    }
}

impl Future for ProgressDone {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, waker: &Waker) -> Poll<Self::Output> {
        let c_arc_mutex_tracker = self.arc_mutex_tracker.clone();
        let mut tracker = c_arc_mutex_tracker.lock().unwrap();
        if tracker.progress_done() {
            Poll::Ready(())
        } else {
            if !self.waker_saved {
                tracker.add_waker(waker.clone());
                self.waker_saved = true;
            }
            Poll::Pending
        }
    }
}

impl<S> SpawnerWait<S> {
    pub fn new(spawner: S) -> Self {
        SpawnerWait {
            spawner,
            arc_mutex_tracker: Arc::new(Mutex::new(Tracker::new())),
        }
    }

    /// Wait until no more progress seems to be possible
    pub fn wait(&self) -> ProgressDone {
        ProgressDone::new(self.arc_mutex_tracker.clone())
    }
}

/// A Waker wrapper, used to create a new wrapper Waker using
/// ArcWake::into_waker() later.
/// Tracks calls to wake().
struct ArcWakerWrapper {
    waker: Waker,
    id: usize,
    arc_mutex_tracker: Arc<Mutex<Tracker>>,
}

impl ArcWakerWrapper {
    pub fn new(waker: Waker, 
               id: usize,
               arc_mutex_tracker: Arc<Mutex<Tracker>>) -> Self {

        ArcWakerWrapper {
            waker,
            id,
            arc_mutex_tracker,
        }
    }
}

impl ArcWake for ArcWakerWrapper {
    fn wake(arc_self: &Arc<Self>) {
        {
            let mut tracker = arc_self.arc_mutex_tracker.lock().unwrap();
            tracker.insert(arc_self.id);
        }
        arc_self.waker.wake()
    }
}

/// Wrap a spawned future
/// The future is given a unique id (For tracking purposes).
/// - We run some code before and after every poll call.
/// - We wrap the Waker passed through the poll calls.
struct FutureWrapper<F> {
    id: usize,
    pin_box_future: Pin<Box<F>>,
    arc_mutex_tracker: Arc<Mutex<Tracker>>,
}

impl<F> FutureWrapper<F> {
    fn new(future: F, 
           arc_mutex_tracker: Arc<Mutex<Tracker>>) -> Self {

        let id = {
            let mut tracker = arc_mutex_tracker.lock().unwrap();
            let id = tracker.next_id();
            tracker.insert(id);
            id
        };

        FutureWrapper {
            id,
            pin_box_future: Box::pin(future),
            arc_mutex_tracker,
        }
    }
}

impl<F> Future for FutureWrapper<F> 
where
    F: Future,
{
    type Output = F::Output;

    fn poll(mut self: Pin<&mut Self>, waker: &Waker) -> Poll<Self::Output> {
        // Remove our task from the pending set:
        {
            let mut tracker = self.arc_mutex_tracker.lock().unwrap();
            tracker.poll_begin();
            tracker.remove(&self.id);
        }

        let arc_waker_wrapper = ArcWakerWrapper::new(waker.clone(), 
                                         self.id,
                                         self.arc_mutex_tracker.clone());
        let waker_wrapper = ArcWake::into_waker(Arc::new(arc_waker_wrapper));
        let res = self.pin_box_future.as_mut().poll(&waker_wrapper);

        // Report that polling is done
        {
            let mut tracker = self.arc_mutex_tracker.lock().unwrap();
            tracker.poll_end();
        }

        res
    }
}

impl<S> Spawn for SpawnerWait<S>
where
    S: Spawn,
{
    fn spawn_obj(
        &mut self, 
        future: FutureObj<'static, ()>
    ) -> Result<(), SpawnError> {

        let arc_mutex_tracker = Arc::clone(&self.arc_mutex_tracker);
        let future_wrapper = FutureWrapper::new(future, arc_mutex_tracker);
        let future_obj = FutureObj::new(Box::pin(future_wrapper));
        self.spawner.spawn_obj(future_obj)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::executor::ThreadPool;
    use futures::task::SpawnExt;
    use futures::{future, StreamExt, SinkExt};
    use futures::channel::mpsc;


    #[test]
    fn test_one_future() {
        let mut thread_pool = ThreadPool::new().unwrap();

        let mut wspawner = SpawnerWait::new(thread_pool.clone());
        let waiter = wspawner.wait();
        wspawner.spawn(future::lazy(|_| ())).unwrap();
        thread_pool.run(waiter);
    }

    #[test]
    fn test_two_futures() {
        let mut thread_pool = ThreadPool::new().unwrap();

        let mut wspawner = SpawnerWait::new(thread_pool.clone());
        let waiter = wspawner.wait();

        let (mut a_sender, mut b_receiver) = mpsc::channel::<u32>(0);
        let (mut b_sender, mut a_receiver) = mpsc::channel::<u32>(0);

        let arc_mutex_res = Arc::new(Mutex::new(false));

        wspawner.spawn(async move {
            await!(a_sender.send(0)).unwrap();
            assert_eq!(await!(a_receiver.next()).unwrap(), 1);
            await!(a_sender.send(2)).unwrap();
        }).unwrap();

        let c_arc_mutex_res = Arc::clone(&arc_mutex_res);
        wspawner.spawn(async move {
            assert_eq!(await!(b_receiver.next()).unwrap(), 0);
            await!(b_sender.send(1)).unwrap();
            assert_eq!(await!(b_receiver.next()).unwrap(), 2);
            let mut res_guard = c_arc_mutex_res.lock().unwrap();
            *res_guard = true;
        }).unwrap();

        thread_pool.run(waiter);
        let res_guard = arc_mutex_res.lock().unwrap();
        assert!(*res_guard);
    }

    #[test]
    fn test_channel_full() {
        let mut thread_pool = ThreadPool::new().unwrap();

        let mut wspawner = SpawnerWait::new(thread_pool.clone());
        let waiter = wspawner.wait();

        let arc_mutex_res = Arc::new(Mutex::new(0usize));

        let c_arc_mutex_res = Arc::clone(&arc_mutex_res);
        wspawner.spawn(async move {
            // Channel has limited capacity:
            let (mut sender, _receiver) = mpsc::channel::<u32>(8);

            // We keep sending into the channel.
            // At some point this loop should be stuck, because the channel is full.
            loop {
                await!(sender.send(0)).unwrap();
                let mut res_guard = c_arc_mutex_res.lock().unwrap();
                *res_guard = res_guard.checked_add(1).unwrap();
            }

        }).unwrap();

        thread_pool.run(waiter);
        let res_guard = arc_mutex_res.lock().unwrap();
        assert_eq!(*res_guard, 9);
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
        fn poll(mut self: Pin<&mut Self>, waker: &Waker) -> Poll<Self::Output> {
            let count = &mut self.as_mut().0;
            *count = count.saturating_sub(1);
            if *count == 0 {
                Poll::Ready(())
            } else {
                waker.wake();
                Poll::Pending
            }
        }
    }

    #[test]
    fn test_yield() {
        let mut thread_pool = ThreadPool::new().unwrap();

        let mut wspawner = SpawnerWait::new(thread_pool.clone());
        let waiter = wspawner.wait();

        for _ in 0 .. 8 {
            wspawner.spawn(Yield::new(0x10)).unwrap();
        }

        thread_pool.run(waiter);
    }

    /*
    // Nested wakers don't seem to work at this point.
    
    #[test]
    fn test_inside_future() {
        let mut thread_pool = ThreadPool::new().unwrap();

        let mut wspawner = SpawnerWait::new(thread_pool.clone());

        let c_wspawner = wspawner.clone();
        let handle = wspawner.spawn_with_handle(c_wspawner.wait()).unwrap();

        thread_pool.run(handle);
    }
    */
}


