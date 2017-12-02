//! The time tick broadcast service.
//!
//! ## Introduction
//!
//! The timer based on broadcast model. It send time tick to all clients
//! periodically.
//!
//! ## The Timer Message Format
//!
//! The message from timer module was shown bellow:
//!
//! ```rust,ignore
//! enum FromTimer {
//!    TimeTick
//! }
//! ```
//!
//! ## Details
//!
//! Currently, timer module backend by [`futures-timer`][futures-timer],
//! which is designed for working with timers, timeouts, and intervals with the
//! [`futures`][futures] crate.
//!
//! Timer module support following operations:
//!
//! - `new(duration: time::Duration) -> TimerModule;`
//! - `create_client(&mut self) -> mpsc::Sender<FromTimer>;`
//!
//! After running the instance of `TimerModule`, paired `Receiver<FromTimer>`
//! returned by [`create_client()`](#method.create_clinet.html) would receive
//! the timer tick periodically, which could be used to run scheduled task and so on.
//!
//! ## Unsolved problem
//!
//! - Graceful shutdown
//!
//! [futures]: https://github.com/alexcrichton/futures
//! [futures-timer]: https://github.com/alexcrichton/futures-timer

extern crate futures_timer;

use std::{time, io, mem};
use futures::{Future, Stream, Poll, Async};
use self::futures_timer::Interval;
use futures::sync::mpsc;

use super::inner_messages::FromTimer;

#[derive(Debug)]
pub enum TimerError {
    IntervalError(io::Error),
}

/// The timer module.
pub struct TimerModule {
    inner: Interval,
    clients: Vec<TimerClient<mpsc::Sender<FromTimer>>>,
}

enum TimerClient<T> {
    Occupied(T),
    Dropped,
}

impl TimerModule {
    pub fn new(duration: time::Duration) -> TimerModule {
        TimerModule {
            inner:   Interval::new(duration),
            clients: Vec::new(),
        }
    }

    pub fn create_client(&mut self) -> mpsc::Receiver<FromTimer> {
        let (sender, receiver) = mpsc::channel(0);
        self.clients.push(TimerClient::Occupied(sender));
        receiver
    }
}

impl Future for TimerModule {
    type Item = ();
    type Error = TimerError;

    fn poll(&mut self) -> Poll<(), TimerError> {
        let poll = self.inner.poll().map_err(|e| {
            TimerError::IntervalError(e)
        })?;

        if poll.is_ready() {
            for ref_client in self.clients.iter_mut() {
                match mem::replace(ref_client, TimerClient::Dropped) {
                    TimerClient::Dropped => (),
                    TimerClient::Occupied(mut sender) => {
                        match sender.try_send(FromTimer::TimeTick) {
                            Err(e) => if e.is_disconnected() {
                                error!("timer client dropped")
                            },
                            _ => {
                                mem::replace(ref_client,
                                             TimerClient::Occupied(sender));
                            },
                        }
                    }
                }
            }
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

        let now = time::Instant::now();
        let client_fut = timer_module.create_client()
            .and_then(move |FromTimer::TimeTick| {
                assert!(now.elapsed() > time::Duration::from_millis(10));
                Ok(())
            }).into_future();

        assert!(core.run(timer_module.map_err(|_| ())
            .select2(client_fut)).is_ok());
    }
}
