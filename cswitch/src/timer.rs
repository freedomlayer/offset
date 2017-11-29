extern crate futures_timer;

use std::time;
use std::sync::Mutex;
use futures::{Future, Stream, Poll, Async};
use self::futures_timer::Interval;
use futures::sync::mpsc;

use super::inner_messages::FromTimer;

/// The timer module.
pub struct TimerModule {
    inner: Interval,
    subscriber: Mutex<Vec<mpsc::Sender<FromTimer>>>,
}

impl TimerModule {
    pub fn new(duration: time::Duration) -> TimerModule {
        TimerModule {
            inner: Interval::new(duration),
            subscriber: Mutex::new(Vec::new()),
        }
    }

    pub fn register(&mut self, tx: mpsc::Sender<FromTimer>) {
        let mut subscriber = self.subscriber.lock().unwrap();
        subscriber.push(tx);
    }

    // TODO: Graceful shutdown
}

impl Future for TimerModule {
    type Item = ();
    type Error = ();

    fn poll(&mut self) -> Poll<(), ()> {
        let poll = self.inner.poll()
            .map_err(|_| {
                error!("internal error");
            })?;

        if poll.is_ready() {
            let mut subscriber = self.subscriber.lock().unwrap();
            subscriber.iter_mut().for_each(|mut tx| {
                // TODO: Error handle
                let _ = tx.try_send(FromTimer::TimeTick).unwrap();
            });
        }

        Ok(Async::NotReady)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use futures::sync::mpsc::channel;

    extern crate tokio_core;

    use self::tokio_core::reactor::Core;

    #[test]
    fn timer_test_basic() {
        let mut timer_module =
            TimerModule::new(time::Duration::from_millis(10));

        let mut core = Core::new().unwrap();
        let handle = core.handle();

        let (tx, rx) = channel::<FromTimer>(1024usize);
        timer_module.register(tx);

        handle.spawn(timer_module);
        thread::sleep(time::Duration::from_millis(10));

        assert!(core.run(rx.and_then(|_| {
            Ok(())
        }).into_future()).is_ok());
    }
}

// TODO: Add more test