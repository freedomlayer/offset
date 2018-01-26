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

#![deny(warnings)]

extern crate futures_timer;

use std::{io, mem, time::Duration};

use futures::prelude::*;
use futures::sync::mpsc;

use self::futures_timer::Interval;

pub mod messages {
    #[derive(Clone)]
    pub enum FromTimer {
        TimeTick,
    }
}

use self::messages::FromTimer;

#[derive(Debug)]
pub enum TimerError {
    Interval(io::Error),
}

/// The timer module.
pub struct TimerModule {
    inner: Interval,
    clients: Vec<TimerClient<mpsc::Sender<FromTimer>>>,
}

#[derive(Debug)]
enum TimerClient<T> {
    Active(T),
    Dropped,
}

impl<T> TimerClient<T> {
    #[inline]
    fn is_dropped(&self) -> bool {
        match *self {
            TimerClient::Active(_) => false,
            TimerClient::Dropped => true,
        }
    }
}

impl TimerModule {
    pub fn new(duration: Duration) -> TimerModule {
        TimerModule {
            inner: Interval::new(duration),
            clients: Vec::new(),
        }
    }

    pub fn create_client(&mut self) -> mpsc::Receiver<FromTimer> {
        let (sender, receiver) = mpsc::channel(100);
        self.clients.push(TimerClient::Active(sender));
        receiver
    }
}

impl Future for TimerModule {
    type Item = ();
    type Error = TimerError;

    fn poll(&mut self) -> Poll<(), TimerError> {
        let timer_poll = self.inner.poll().map_err(TimerError::Interval);

        if try_ready!(timer_poll).is_some() {
            for client in &mut self.clients {
                match mem::replace(client, TimerClient::Dropped) {
                    TimerClient::Dropped => {
                        unreachable!("encounter a dropped client");
                    }
                    TimerClient::Active(mut sender) => {
                        match sender.start_send(FromTimer::TimeTick) {
                            Err(_e) => {
                                info!("timer client disconnected");
                            }
                            Ok(start_send) => {
                                match start_send {
                                    AsyncSink::Ready => {
                                        // For now, this should always succeed
                                        if sender.poll_complete().is_ok() {
                                            mem::replace(client, TimerClient::Active(sender));
                                        }
                                    }
                                    AsyncSink::NotReady(_) => {
                                        warn!("failed to send tick");
                                    }
                                }
                            }
                        }
                    }
                }
            }
            // Remove the dropped clients
            self.clients.retain(|client| !client.is_dropped());

            Ok(Async::NotReady)
        } else {
            Ok(Async::Ready(()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::time;

    use futures::future::join_all;
    use tokio_core::reactor::Core;

    #[test]
    fn test_timer_basic() {
        let mut tm = TimerModule::new(Duration::from_millis(5));

        let mut core = Core::new().unwrap();

        let clients = (0..50)
            .map(|_| tm.create_client())
            .step_by(2)
            .collect::<Vec<_>>();

        assert_eq!(clients.len(), 25);

        let now = time::Instant::now();
        let clients_fut = clients
            .into_iter()
            .map(move |timer_client| {
                let mut ticks: u64 = 0;
                timer_client.take(1000).for_each(move |FromTimer::TimeTick| {
                    ticks += 1;
                    // Acceptable accuracy for test: 5ms (+- 40%)
                    assert!(now.elapsed() > Duration::from_millis(3 * ticks));
                    assert!(now.elapsed() < Duration::from_millis(7 * ticks));
                    Ok(())
                })
            })
            .collect::<Vec<_>>();

        let task = tm.map_err(|_| ()).select2(join_all(clients_fut));

        assert!(core.run(task).is_ok());
    }
}
