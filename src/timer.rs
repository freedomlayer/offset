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
//! ```rust,norun
//! enum FromTimer {
//!     TimeTick,
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

use std::{io, mem, time::Duration};
use futures::prelude::*;
use futures::sync::mpsc;

use tokio_core::reactor::{Handle, Interval};

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
    clients: Vec<Option<mpsc::Sender<FromTimer>>>,
}

impl TimerModule {
    pub fn new(dur: Duration, handle: &Handle) -> TimerModule {
        let inner = Interval::new(dur, handle).expect("can't create timer module");

        TimerModule {
            inner,
            clients: Vec::new(),
        }
    }

    pub fn create_client(&mut self) -> mpsc::Receiver<FromTimer> {
        let (sender, receiver) = mpsc::channel(0);
        self.clients.push(Some(sender));
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
                match mem::replace(client, None) {
                    None => {
                        unreachable!("encounter a dropped client");
                    }
                    Some(mut sender) => {
                        match sender.start_send(FromTimer::TimeTick) {
                            Err(_e) => {
                                info!("timer client disconnected");
                            }
                            Ok(start_send) => {
                                match start_send {
                                    AsyncSink::Ready => {
                                        // For now, this should always succeed
                                        if sender.poll_complete().is_ok() {
                                            mem::replace(client, Some(sender));
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
            self.clients.retain(|client| client.is_some());

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
        let mut core = Core::new().unwrap();
        let handle = core.handle();

        let dur = Duration::from_millis(10);

        let mut tm = TimerModule::new(dur, &handle);

        const TICKS: u32 = 20;
        const TIMER_CLIENT_NUM: usize = 50;

        let clients = (0..TIMER_CLIENT_NUM)
            .map(|_| tm.create_client())
            .step_by(2)
            .collect::<Vec<_>>();

        assert_eq!(clients.len(), TIMER_CLIENT_NUM / 2);

        let start = time::Instant::now();
        let clients_fut = clients
            .into_iter()
            .map(|client| client.take(u64::from(TICKS)).collect().and_then(|_| {
                    assert!(start.elapsed() >= dur * TICKS);
                    assert!(start.elapsed() < dur * TICKS * 15 / 10);
                    Ok(())
            }))
            .collect::<Vec<_>>();

        let task = tm.map_err(|_| ()).select2(join_all(clients_fut));

        assert!(core.run(task).is_ok());
    }
}
