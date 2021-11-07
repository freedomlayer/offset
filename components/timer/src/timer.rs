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

#![allow(broken_intra_doc_links)]
#![allow(clippy::implicit_hasher, clippy::module_inception)]

use common::conn::BoxStream;
use common::select_streams::select_streams;

use async_std::stream::interval;

use futures::channel::{mpsc, oneshot};
use futures::prelude::*;
use futures::stream;
use futures::task::{Spawn, SpawnExt};
use std::time::Duration;

#[derive(Debug, Eq, PartialEq)]
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
    /// Name of requesting task, used for debugging purposes
    task_name: String,
    response_sender: oneshot::Sender<mpsc::Receiver<TimerTick>>,
}

impl std::fmt::Debug for TimerRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "TimerRequest{{task_name: {}}}", self.task_name)
    }
}

#[derive(Clone, Debug)]
pub struct TimerClient {
    sender: mpsc::Sender<TimerRequest>,
}

impl TimerClient {
    fn new(sender: mpsc::Sender<TimerRequest>) -> TimerClient {
        TimerClient { sender }
    }

    pub async fn request_timer_stream(
        &mut self,
        task_name: String,
    ) -> Result<mpsc::Receiver<TimerTick>, TimerClientError> {
        let (response_sender, response_receiver) = oneshot::channel();
        let timer_request = TimerRequest {
            task_name,
            response_sender,
        };
        self.sender
            .send(timer_request)
            .await
            .map_err(|_| TimerClientError::SendFailure)?;

        match response_receiver.await {
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

// TODO: Possibly give to timer_loop as an argument?
/// Size of queue for pending ticks for a single client.
/// After `TICK_QUEUE_LEN` ticks, the timer will not be able to queue more ticks to a nonresponsive
/// client.
const TICK_QUEUE_LEN: usize = 8;

#[derive(Debug)]
struct ClientEntry {
    task_name: String,
    sender: mpsc::Sender<TimerTick>,
}

async fn timer_loop<M>(
    incoming: M,
    from_client: mpsc::Receiver<TimerRequest>,
) -> Result<(), TimerError>
where
    M: Stream<Item = ()> + std::marker::Unpin + Send,
{
    let incoming = incoming
        .map(|_| TimerEvent::Incoming)
        .chain(stream::once(future::ready(TimerEvent::IncomingDone)));
    let from_client = from_client
        .map(TimerEvent::Request)
        .chain(stream::once(future::ready(TimerEvent::RequestsDone)));

    // TODO: What happens if one of the two streams (incoming, from_client) is closed?
    let mut events = select_streams![incoming, from_client];
    // let mut tick_senders: Vec<mpsc::Sender<TimerTick>> = Vec::new();
    let mut client_entries: Vec<ClientEntry> = Vec::new();
    let mut requests_done = false;

    while let Some(event) = events.next().await {
        match event {
            TimerEvent::Incoming => {
                let mut temp_client_entries = Vec::new();
                temp_client_entries.append(&mut client_entries);
                for mut client_entry in temp_client_entries {
                    match client_entry.sender.try_send(TimerTick) {
                        Ok(()) => client_entries.push(client_entry),
                        Err(e) => {
                            // In case of error, we disconnect client
                            if !e.is_disconnected() {
                                // Error trying to send a tick to client.
                                // Client might be too busy?
                                error!(
                                    "timer_loop(): try_send() error. task_name: {}, error: {:?}",
                                    client_entry.task_name, e
                                );
                            }
                        }
                    }
                }
            }
            TimerEvent::Request(timer_request) => {
                let (tick_sender, tick_receiver) = mpsc::channel(TICK_QUEUE_LEN);
                client_entries.push(ClientEntry {
                    task_name: timer_request.task_name,
                    sender: tick_sender,
                });
                let _ = timer_request.response_sender.send(tick_receiver);
            }
            TimerEvent::IncomingDone => {
                break;
            }
            TimerEvent::RequestsDone => {
                requests_done = true;
            }
        };
        if requests_done && client_entries.is_empty() {
            break;
        }
    }
    Ok(())
}

/// Create a timer service that broadcasts everything from the incoming Stream.
/// Useful for testing, as this function allows full control on the rate of incoming signals.
pub fn create_timer_incoming<M>(incoming: M, spawner: impl Spawn) -> Result<TimerClient, TimerError>
where
    M: Stream<Item = ()> + std::marker::Unpin + Send + 'static,
{
    let (sender, receiver) = mpsc::channel::<TimerRequest>(0);
    let timer_loop_future = timer_loop(incoming, receiver);
    let total_fut = timer_loop_future
        .map_err(|e| error!("timer loop error: {:?}", e))
        .then(|_| future::ready(()));
    spawner.spawn(total_fut).unwrap();
    Ok(TimerClient::new(sender))
}

/// A test util function. Every time a timer_client.request_timer_stream() is called,
/// a new mpsc::Sender<TimerTick> will be received through the receiver.
/// This provides greater control over the sent timer ticks.
pub fn dummy_timer_multi_sender(
    spawner: impl Spawn,
) -> (mpsc::Receiver<mpsc::Sender<TimerTick>>, TimerClient) {
    let (request_sender, mut request_receiver) = mpsc::channel::<TimerRequest>(0);
    let (mut tick_sender_sender, tick_sender_receiver) = mpsc::channel(1);
    spawner
        .spawn(async move {
            while let Some(timer_request) = request_receiver.next().await {
                let (tick_sender, tick_receiver) = mpsc::channel::<TimerTick>(0);

                tick_sender_sender.send(tick_sender).await.unwrap();
                timer_request.response_sender.send(tick_receiver).unwrap();
            }
        })
        .unwrap();

    (tick_sender_receiver, TimerClient::new(request_sender))
}

/// Create a timer service that ticks every `dur`.
pub fn create_timer(dur: Duration, spawner: impl Spawn) -> Result<TimerClient, TimerError> {
    let interval = interval(dur);
    create_timer_incoming(interval, spawner)
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::pin::Pin;
    use futures::executor::{block_on, LocalPool, ThreadPool};
    use futures::future::join;
    use std::time::{Duration, Instant};

    #[test]
    fn test_timer_single() {
        let thread_pool = ThreadPool::new().unwrap();

        let dur = Duration::from_millis(1);
        let timer_client = create_timer(dur, thread_pool.clone()).unwrap();

        let timer_stream = LocalPool::new()
            .run_until(
                timer_client
                    .clone()
                    .request_timer_stream("test_timer_single".to_owned()),
            )
            .unwrap();
        let wait_fut = timer_stream.take(10).collect::<Vec<TimerTick>>();
        block_on(wait_fut);
    }

    #[test]
    fn test_timer_twice() {
        let thread_pool = ThreadPool::new().unwrap();

        let dur = Duration::from_millis(1);
        let timer_client = create_timer(dur, thread_pool.clone()).unwrap();

        let timer_stream = LocalPool::new()
            .run_until(
                timer_client
                    .clone()
                    .request_timer_stream("test_timer_twice_0".to_owned()),
            )
            .unwrap();
        let wait_fut = timer_stream.take(10).collect::<Vec<TimerTick>>();
        block_on(wait_fut);

        let timer_stream = LocalPool::new()
            .run_until(
                timer_client
                    .clone()
                    .request_timer_stream("test_timer_twice_1".to_owned()),
            )
            .unwrap();
        let wait_fut = timer_stream.take(10).collect::<Vec<TimerTick>>();
        block_on(wait_fut);
    }

    #[test]
    fn test_timer_create_multiple_streams() {
        let thread_pool = ThreadPool::new().unwrap();

        // Create a mock time service:
        let (_tick_sender, tick_receiver) = mpsc::channel::<()>(0);
        let timer_client = create_timer_incoming(tick_receiver, thread_pool.clone()).unwrap();

        let mut timer_streams = Vec::new();
        for _ in 0..10 {
            let timer_stream = block_on(
                timer_client
                    .clone()
                    .request_timer_stream("test_timer_create_multiple_streams".to_owned()),
            )
            .unwrap();
            timer_streams.push(timer_stream);
        }
    }

    #[test]
    fn test_timer_multiple() {
        let thread_pool = ThreadPool::new().unwrap();

        let dur = Duration::from_millis(10);
        let timer_client = create_timer(dur, thread_pool.clone()).unwrap();

        const TICKS: u32 = 4;
        const TIMER_CLIENT_NUM: usize = 2;

        let mut senders = Vec::new();
        let mut joined_receivers =
            Box::pin(future::ready(())) as Pin<Box<dyn Future<Output = ()> + Send>>;

        for _ in 0..TIMER_CLIENT_NUM {
            let (sender, receiver) = oneshot::channel::<()>();
            senders.push(sender);

            let receiver = receiver.map(|_| ());

            let new_join = join(joined_receivers, receiver).map(|_| ());
            joined_receivers = Box::pin(new_join) as Pin<Box<dyn Future<Output = ()> + Send>>;
        }

        let (sender_done, receiver_done) = oneshot::channel::<()>();

        thread_pool
            .spawn(joined_receivers.map(|_| sender_done.send(()).unwrap()))
            .unwrap();

        let start = Instant::now();
        for _ in 0..TIMER_CLIENT_NUM {
            let sender = senders.pop().unwrap();
            let new_client = LocalPool::new()
                .run_until(
                    timer_client
                        .clone()
                        .request_timer_stream("test_timer_multiple".to_owned()),
                )
                .unwrap();
            let client_wait = new_client.take(TICKS as usize).collect::<Vec<TimerTick>>();
            let client_fut = client_wait.map(move |_| {
                let elapsed = start.elapsed();
                assert!(elapsed >= dur * TICKS * 2 / 3);
                assert!(elapsed < dur * TICKS * 2);
                sender.send(()).unwrap();
                ()
            });

            thread_pool.spawn(client_fut).unwrap();
        }

        block_on(receiver_done).unwrap();
    }

    /////////////////////////////////////////////////////////////////////////////////////

    #[derive(Debug, Eq, PartialEq)]
    enum ReadError {
        Closed,
    }

    /// Util function to read from a Stream
    async fn receive<T, M: 'static>(reader: M) -> Result<(T, M), ReadError>
    where
        M: Stream<Item = T> + std::marker::Unpin,
    {
        let (opt_reader_message, ret_reader) = reader.into_future().await;
        match opt_reader_message {
            Some(reader_message) => Ok((reader_message, ret_reader)),
            None => return Err(ReadError::Closed),
        }
    }

    struct CustomTick;

    async fn task_ticks_receiver<S>(
        mut tick_sender: S,
        mut timer_client: TimerClient,
    ) -> Result<(), ()>
    where
        S: Sink<(), Error = ()> + std::marker::Unpin + 'static,
    {
        let timer_stream = timer_client
            .request_timer_stream("task_ticks_receiver".to_owned())
            .await
            .unwrap();
        let mut timer_stream = timer_stream.map(|_| CustomTick);
        for _ in 0..8usize {
            tick_sender.send(()).await.unwrap();
            let (_, new_timer_stream) = receive(timer_stream).await.unwrap();
            timer_stream = new_timer_stream;
        }
        drop(tick_sender);
        match receive(timer_stream).await {
            Ok(_) => unreachable!(),
            Err(e) => assert_eq!(e, ReadError::Closed),
        };

        Ok(())
    }

    #[test]
    fn test_create_timer_incoming() {
        let thread_pool = ThreadPool::new().unwrap();

        // Create a mock time service:
        let (tick_sender, tick_receiver) = mpsc::channel::<()>(0);
        let timer_client = create_timer_incoming(tick_receiver, thread_pool.clone()).unwrap();

        let tick_sender = tick_sender.sink_map_err(|_| ());
        block_on(task_ticks_receiver(tick_sender, timer_client)).unwrap();
    }

    async fn task_dummy_timer_multi_sender(spawner: impl Spawn) {
        let (mut tick_sender_receiver, mut timer_client) = dummy_timer_multi_sender(spawner);

        let timer_stream_fut = async {
            let mut timer_stream = timer_client
                .request_timer_stream("task_dummy_timer_multi_sender".to_owned())
                .await
                .unwrap();
            for _ in 0..16usize {
                assert_eq!(timer_stream.next().await, Some(TimerTick));
            }
            assert_eq!(timer_stream.next().await, None);
        };
        let tick_sender_fut = async {
            let mut tick_sender = tick_sender_receiver.next().await.unwrap();

            for _ in 0..16usize {
                tick_sender.send(TimerTick).await.unwrap();
            }
        };
        let _ = join(timer_stream_fut, tick_sender_fut).await;
    }

    #[test]
    fn test_dummy_timer_multi_sender() {
        let thread_pool = ThreadPool::new().unwrap();
        block_on(task_dummy_timer_multi_sender(thread_pool.clone()));
    }
}
