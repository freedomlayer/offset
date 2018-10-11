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

use std::time::{Duration, Instant};
use futures::prelude::*;
use futures::channel::{mpsc, oneshot};
use futures::task::{Spawn, SpawnExt};
use futures::stream;
use futures_util::compat::{Stream01CompatExt};
use tokio_timer::Interval;


#[derive(Debug)]
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

impl std::fmt::Debug for TimerRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "TimerRequest")
    }
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

    pub async fn request_timer_stream(mut self) -> Result<mpsc::Receiver<TimerTick>, TimerClientError> {
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

#[derive(Debug)]
enum TimerEvent {
    Incoming,
    IncomingDone,
    Request(TimerRequest),
    RequestsDone,
}

async fn timer_loop<M>(incoming: M, from_client: mpsc::Receiver<TimerRequest>) -> Result<(), TimerError> 
where
    M: Stream<Item=()> + std::marker::Unpin,
{
    let incoming = incoming.map(|_| TimerEvent::Incoming)
        .chain(stream::once(future::ready(TimerEvent::IncomingDone)));
    let from_client = from_client.map(|timer_request| TimerEvent::Request(timer_request))
        .chain(stream::once(future::ready(TimerEvent::RequestsDone)));

    // TODO: What happens if one of the two streams (incoming, from_client) is closed?
    let mut events = incoming.select(from_client);
    let mut tick_senders: Vec<mpsc::Sender<TimerTick>> = Vec::new();
    let mut requests_done = false;

    while let Some(event) = await!(events.next()) {
        // dbg!(tick_senders.len());
        match event {
            TimerEvent::Incoming => {
                let mut temp_tick_senders = Vec::new();
                temp_tick_senders.append(&mut tick_senders);
                for mut tick_sender in temp_tick_senders {
                    if let Ok(()) = await!(tick_sender.send(TimerTick)) {
                        dbg!("Timer sending tick");
                        tick_senders.push(tick_sender);
                    }
                }
            },
            TimerEvent::Request(timer_request) => {
                println!("Timer: Got request");
                let (tick_sender, tick_receiver) = mpsc::channel(0);
                tick_senders.push(tick_sender);
                let _ = dbg!(timer_request.response_sender.send(tick_receiver));
            },
            TimerEvent::IncomingDone => {
                println!("IncomingDone!");
                break;
            },
            TimerEvent::RequestsDone => {
                println!("RequestsDone!");
                requests_done = true;
            },
        };
        if requests_done && tick_senders.is_empty() {
            break;
        }
    }
    dbg!("Timer exited!");
    Ok(())
}

/// Create a timer service that broadcasts everything from the incoming Stream.
/// Useful for testing, as this function allows full control on the rate of incoming signals.
pub fn create_timer_incoming<M>(incoming: M, mut spawner: impl Spawn) -> Result<TimerClient, TimerError> 
where
    M: Stream<Item=()> + std::marker::Unpin + std::marker::Send + 'static,
{
    let (sender, receiver) = mpsc::channel::<TimerRequest>(0);
    let timer_loop_future = timer_loop(incoming, receiver);
    let total_fut = timer_loop_future
        .map_err(|e| error!("timer loop error: {:?}",e))
        .then(|_| future::ready(()));
    spawner.spawn(total_fut).unwrap();
    Ok(TimerClient::new(sender))
}

/// Create a timer service that ticks every `dur`.
pub fn create_timer(dur: Duration, spawner: impl Spawn) -> Result<TimerClient, TimerError> {

    let interval = Interval::new(Instant::now() + dur, dur)
        .compat()
        .map(|_| ());

    create_timer_incoming(interval, spawner)
}



#[cfg(test)]
mod tests {
    use super::*;
    use std::time;
    use futures::executor::LocalPool;
    use futures::future::FutureObj;
    use tokio_timer::Timeout;
    use futures_util::compat::{Future01CompatExt};
    use futures_01::Future;
    // use core::pin::Pin;

    #[test]
    fn test_timer_single() {
        let mut local_pool = LocalPool::new();
        let spawner = local_pool.spawner();

        let dur = Duration::from_millis(10);
        let timer_client = create_timer(dur, spawner).unwrap();

        let timer_stream = local_pool.run_until(timer_client.clone().request_timer_stream()).unwrap();
        let wait_fut = timer_stream.take(10).collect::<Vec<TimerTick>>();
        local_pool.run_until(wait_fut);
    }

    #[test]
    fn test_timeout() {
        let mut local_pool = LocalPool::new();
        let spawner = local_pool.spawner();

        let my_timeout = Timeout::new((), Duration::from_millis(1000)).compat();
        /*
        let my_fut = my_timeout
            .map(|_| {
                println!("Hello!");
            });
            */
        local_pool.run_until(my_timeout);

        /*
        let dur = Duration::new(1,0);
        let interval = Interval::new(Instant::now(), dur)
            .compat()
            .map(|_| ());

        let my_fut = interval.for_each(|_| {
            println!("Interval tick!");
            future::ready(())
        });
        local_pool.run_until(my_fut);
        */
    }

    #[test]
    fn test_timer_twice() {
        let mut local_pool = LocalPool::new();
        let spawner = local_pool.spawner();

        let dur = Duration::from_millis(100);
        let timer_client = create_timer(dur, spawner).unwrap();

        let timer_stream = local_pool.run_until(timer_client.clone().request_timer_stream()).unwrap();
        let wait_fut = timer_stream.take(10).collect::<Vec<TimerTick>>();
        local_pool.run_until(wait_fut);

        let timer_stream = local_pool.run_until(timer_client.clone().request_timer_stream()).unwrap();
        let wait_fut = timer_stream.take(10).collect::<Vec<TimerTick>>();
        local_pool.run_until(wait_fut);
    }

    #[test]
    fn test_timer_create_multiple_streams() {
        let mut local_pool = LocalPool::new();
        let spawner = local_pool.spawner();

        // Create a mock time service:
        let (_tick_sender, tick_receiver) = mpsc::channel::<()>(0);
        let timer_client = create_timer_incoming(tick_receiver, spawner).unwrap();

        let mut timer_streams = Vec::new();
        for _ in 0 .. 10 {
            let timer_stream = local_pool.run_until(timer_client.clone().request_timer_stream()).unwrap();
            timer_streams.push(timer_stream);
        }
    }

    #[test]
    fn test_timer_multiple() {
        let mut local_pool = LocalPool::new();
        let mut spawner = local_pool.spawner();

        let dur = Duration::from_millis(10);
        let timer_client = create_timer(dur, spawner.clone()).unwrap();

        const TICKS: u32 = 4;
        const TIMER_CLIENT_NUM: usize = 2;

        let mut senders = Vec::new();
        let mut joined_receivers = FutureObj::from(future::ready(()).boxed());

        for _ in 0 .. TIMER_CLIENT_NUM {
            let (sender, receiver) = oneshot::channel::<()>();
            senders.push(sender);

            let receiver = receiver.map(|_| {
                println!("Receiver done!");
                ()
            });

            let new_join = joined_receivers.join(receiver).map(|_| ());
            joined_receivers = FutureObj::from(new_join.boxed());
        }

        let (sender_done, receiver_done) = oneshot::channel::<()>();

        spawner.spawn(joined_receivers.map(|_| sender_done.send(()).unwrap())).unwrap();

        let start = time::Instant::now();
        for i in 0 .. TIMER_CLIENT_NUM {
            let sender = senders.pop().unwrap();
            println!("Creating client number {} ...",i);
            let new_client = local_pool.run_until(timer_client.clone().request_timer_stream()).unwrap();
            println!("Client number {}", i);
            let client_wait = new_client.take(u64::from(TICKS)).collect::<Vec<TimerTick>>();
            let client_fut = client_wait.map(move |_| {
                // let elapsed = start.elapsed();
                // assert!(elapsed >= dur * TICKS * 2 / 3);
                // assert!(elapsed < dur * TICKS * 2);
                sender.send(()).unwrap();
                println!("Client {} done!",i);
                ()
            });

            spawner.spawn(client_fut).unwrap();
        }

        local_pool.run_until(receiver_done).unwrap();
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
        let mut local_pool = LocalPool::new();
        let spawner = local_pool.spawner();

        // Create a mock time service:
        let (tick_sender, tick_receiver) = mpsc::channel::<()>(0);
        let timer_client = create_timer_incoming(tick_receiver, spawner).unwrap();

        let tick_sender = tick_sender.sink_map_err(|_| ());
        local_pool.run_until(task_ticks_receiver(tick_sender, timer_client)).unwrap();

    }
}
