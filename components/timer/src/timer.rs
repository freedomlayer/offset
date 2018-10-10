//! The time tick broadcast service.
//!
//! ## Introduction
//!
//! The timer is based on broadcast model. It sends time tick to all clients
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

use std::{time::Duration};
use futures::prelude::*;
use futures::channel::{mpsc, oneshot};
use futures::task::Spawn;
use futures::stream;

pub struct TimerTick;

#[derive(Debug)]
pub enum TimerError {
    IntervalCreationError,
    IncomingError,
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

    pub async fn request_timer_stream(self) -> Result<mpsc::Receiver<TimerTick>, TimerClientError> {
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
    Incoming,
    IncomingDone,
    Request(TimerRequest),
    RequestsDone,
}

async fn timer_loop<M: 'static>(incoming: M, from_client: mpsc::Receiver<TimerRequest>) -> Result<(), TimerError> 
where
    M: Stream<Item=()>,
{
    let incoming = incoming.map(|_| TimerEvent::Incoming)
        .chain(stream::once(future::ready(TimerEvent::IncomingDone)));
    let from_client = from_client.map(|timer_request| TimerEvent::Request(timer_request))
        .chain(stream::once(future::ready(TimerEvent::RequestsDone)));

    // TODO: What happens if one of the two streams (incoming, from_client) is closed?
    let events = incoming.select(from_client);
    let mut tick_senders: Vec<mpsc::Sender<TimerTick>> = Vec::new();
    let mut requests_done = false;

    events.take_while(|event| {
        match event {
            TimerEvent::Incoming => {
                let mut temp_tick_senders = Vec::new();
                temp_tick_senders.append(&mut tick_senders);
                for tick_sender in temp_tick_senders {
                    if let Ok(()) = await!(tick_sender.send(TimerTick)) {
                        tick_senders.push(tick_sender);
                    }
                }
            },
            TimerEvent::Request(timer_request) => {
                let (tick_sender, tick_receiver) = mpsc::channel(0);
                tick_senders.push(tick_sender);
                let _ = timer_request.response_sender.send(tick_receiver);
            },
            TimerEvent::IncomingDone => return future::ready(false),
            TimerEvent::RequestsDone => requests_done = true,
        };
        if requests_done && tick_senders.is_empty() {
            return future::ready(false);
        }
        future::ready(true)
    });
    Ok(())
}

/// Create a timer service that broadcasts everything from the incoming Stream.
/// Useful for testing, as this function allows full control on the rate of incoming signals.
pub fn create_timer_incoming<M: 'static>(incoming: M, spawner: impl Spawn) -> Result<TimerClient, TimerError> 
where
    M: Stream<Item=()>,
{
    let (sender, receiver) = mpsc::channel::<TimerRequest>(0);
    let timer_loop_future = timer_loop(incoming, receiver);
    spawner.spawn(timer_loop_future.then(|_| Ok(())));
    Ok(TimerClient::new(sender))
}

/// Create a timer service that ticks every `dur`.
pub fn create_timer(dur: Duration, spawner: impl Spawn) -> Result<TimerClient, TimerError> {
    let interval = Interval::new(dur, spawner)
        .map_err(|_| TimerError::IntervalCreationError)?;

    create_timer_incoming(interval.map_err(|_| ()), spawner)
}



#[cfg(test)]
mod tests {
    use super::*;
    use std::time;
    use futures::task::Spawn;
    use futures::executor::LocalPool;

    #[test]
    fn test_timer_basic() {
        let mut local_pool = LocalPool::new();
        let spawner = local_pool.spawner();


        let dur = Duration::from_millis(10);
        let timer_client = create_timer(dur, spawner).unwrap();

        const TICKS: u32 = 20;
        const TIMER_CLIENT_NUM: usize = 50;

        let clients = (0..TIMER_CLIENT_NUM)
            .map(|_| local_pool.run_until(timer_client.clone().request_timer_stream()).unwrap())
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

    /////////////////////////////////////////////////////////////////////////////////////


    #[derive(Debug, Eq, PartialEq)]
    enum ReadError {
        Closed,
        Error,
    }

    /// Util function to read from a Stream
    async fn receive<T, M: 'static>(reader: M) -> Result<(T, M), ReadError>
        where M: Stream<Item=T> + std::marker::Unpin,
    {
        let (opt_reader_message, ret_reader) = await!(reader.into_future());
        match opt_reader_message {
            Some(reader_message) => Ok((reader_message, ret_reader)),
            None => return Err(ReadError::Closed),
        }
    }

    struct CustomTick;

    async fn task_ticks_receiver<S>(mut tick_sender: S,
                           timer_client: TimerClient) -> Result<(), ()> 
    where
        S: Sink<SinkItem=(), SinkError=()> + std::marker::Unpin + 'static
    {
        let timer_stream = await!(timer_client.request_timer_stream()).unwrap();
        let mut timer_stream = timer_stream.map(|_| CustomTick);
        for _ in 0 .. 8usize {
            await!(tick_sender.send(())).unwrap();
            let (_, new_timer_stream) = await!(receive(timer_stream)).unwrap();
            timer_stream = new_timer_stream;
        }
        drop(tick_sender);
        match await!(receive(timer_stream)) {
            Ok(_) => unreachable!(),
            Err(e) => assert_eq!(e, ReadError::Closed),
        };

        Ok(())
    }

    #[test]
    fn test_create_timer_incoming() {
        let mut core = Core::new().unwrap();
        let handle = core.handle();

        // Create a mock time service:
        let (tick_sender, tick_receiver) = mpsc::channel::<()>(0);
        let timer_client = create_timer_incoming(tick_receiver, &handle).unwrap();

        let tick_sender = tick_sender.sink_map_err(|_| ());
        core.run(task_ticks_receiver(tick_sender, timer_client)).unwrap();

    }
}
