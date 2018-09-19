//! The time tick broadcast service.
//!
//! ## Introduction
//!
//! The timer based on broadcast model. It send time tick to all clients
//! periodically.
//!
//! ## The Timer Message Format
//!
//! ## Details
//!
//! Currently, timer module backend by [`futures-timer`][futures-timer],
//! which is designed for working with timers, timeouts, and intervals with the
//! [`futures`][futures] crate.
//!
//! ## Unsolved problem
//!
//! - Graceful shutdown
//!
//! [futures]: https://github.com/alexcrichton/futures
//! [futures-timer]: https://github.com/alexcrichton/futures-timer

// #![deny(warnings)]

use std::{io, time::Duration};
use futures::prelude::*;
use futures::prelude::{async, await};
use futures::sync::{mpsc, oneshot};

use tokio_core::reactor::{Handle, Interval};

pub struct TimerTick;

#[derive(Debug)]
pub enum TimerError {
    IntervalCreationError,
    Interval(io::Error),
    IncomingRequestsClosed,
    IncomingRequestsError,
}

#[derive(Debug)]
pub enum TimerClientError {
    SendFailure,
    ResponseCanceled,
}

struct TimerRequest {
    response_sender: oneshot::Sender<mpsc::Receiver<TimerTick>>,
}

#[derive(Clone)]
pub struct TimerClient {
    sender: mpsc::Sender<TimerRequest>,
}

impl TimerClient {
    fn new(sender: mpsc::Sender<TimerRequest>) -> TimerClient {
        TimerClient {
            sender
        }
    }

    #[async]
    pub fn request_timer_stream(self) -> Result<mpsc::Receiver<TimerTick>, TimerClientError> {
        let (response_sender, response_receiver) = oneshot::channel();
        let timer_request = TimerRequest { response_sender };
        let _ = match await!(self.sender.send(timer_request)) {
            Ok(sender) => Ok(sender),
            Err(_) => Err(TimerClientError::SendFailure),
        }?;
        match await!(response_receiver) {
            Ok(timer_stream) => Ok(timer_stream),
            Err(_) => Err(TimerClientError::ResponseCanceled),
        }
    }
}

enum TimerEvent {
    Interval,
    IncomingRequest(TimerRequest),
}

#[async]
fn timer_loop(interval: Interval, incoming: mpsc::Receiver<TimerRequest>) -> Result<!, TimerError> {
    let interval = interval.map(|_| TimerEvent::Interval)
        .map_err(TimerError::Interval);
    let incoming = incoming.map(|timer_request| TimerEvent::IncomingRequest(timer_request))
        .map_err(|_| TimerError::IncomingRequestsError);

    // TODO: What happens if one of the two streams (interval, incoming) is closed?
    let events = interval.select(incoming);
    let mut tick_senders: Vec<mpsc::Sender<TimerTick>> = Vec::new();

    #[async]
    for event in events {
        match event {
            TimerEvent::Interval => {
                let mut temp_tick_senders = Vec::new();
                temp_tick_senders.append(&mut tick_senders);
                for tick_sender in temp_tick_senders {
                    if let Ok(tick_sender) = await!(tick_sender.send(TimerTick)) {
                        tick_senders.push(tick_sender);
                    }
                }
            },
            TimerEvent::IncomingRequest(timer_request) => {
                let (tick_sender, tick_receiver) = mpsc::channel(0);
                tick_senders.push(tick_sender);
                let _ = timer_request.response_sender.send(tick_receiver);
            },
        }
    }

    Err(TimerError::IncomingRequestsClosed)
}

pub fn create_timer(dur: Duration, handle: &Handle) -> Result<TimerClient, TimerError> {
    let interval = Interval::new(dur, handle)
        .map_err(|_| TimerError::IntervalCreationError)?;

    let (sender, receiver) = mpsc::channel::<TimerRequest>(0);
    let timer_loop_future = timer_loop(interval, receiver);
    handle.spawn(timer_loop_future.then(|_| Ok(())));
    Ok(TimerClient::new(sender))
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
        let timer_client = create_timer(dur, &handle).unwrap();

        const TICKS: u32 = 20;
        const TIMER_CLIENT_NUM: usize = 50;

        let clients = (0..TIMER_CLIENT_NUM)
            .map(|_| core.run(timer_client.clone().request_timer_stream()).unwrap())
            .step_by(2)
            .collect::<Vec<_>>();

        assert_eq!(clients.len(), TIMER_CLIENT_NUM / 2);

        let start = time::Instant::now();
        let clients_fut = clients
            .into_iter()
            .map(|client| client.take(u64::from(TICKS)).collect().and_then(|_| {
                    let elapsed = start.elapsed();
                    assert!(elapsed >= dur * TICKS * 2 / 3);
                    assert!(elapsed < dur * TICKS * 2);
                    Ok(())
            }))
            .collect::<Vec<_>>();

        let task = join_all(clients_fut);

        assert!(core.run(task).is_ok());
    }
}
