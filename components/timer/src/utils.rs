use std::marker::Unpin;

use crate::timer::{TimerClient, TimerTick};
use futures::select;
use futures::{future, Future, FutureExt, Stream, StreamExt};

#[derive(Debug)]
pub enum SleepTicksError {
    RequestTimerStreamError,
}

/// Sleep for a certain amount of time ticks
pub async fn sleep_ticks(
    ticks: usize,
    mut timer_client: TimerClient,
) -> Result<(), SleepTicksError> {
    let timer_stream = timer_client
        .request_timer_stream()
        .await
        .map_err(|_| SleepTicksError::RequestTimerStreamError)?;
    let fut = timer_stream.take(ticks).for_each(|_| future::ready(()));
    fut.await;
    Ok(())
}

/// Wraps a future with a timeout.
/// If the future finishes before the timeout with value v, Some(v) is returned.
/// Otherwise, None is returned.
pub async fn future_timeout<T, F, TS>(fut: F, timer_stream: TS, time_ticks: usize) -> Option<T>
where
    TS: Stream<Item = TimerTick> + Unpin + Send + 'static,
    F: Future<Output = T> + Unpin,
{
    let mut fut_time = timer_stream
        .take(time_ticks)
        .for_each(|_| future::ready(()))
        .map(|_| None);

    select! {
        fut = fut.fuse() => Some(fut),
        fut_time = fut_time => fut_time,
    }
}

// TODO: Add tests.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::timer::create_timer_incoming;
    use futures::channel::{mpsc, oneshot};
    use futures::executor::{ThreadPool, LocalPool};
    use futures::task::{Spawn, SpawnExt};
    use futures::SinkExt;

    async fn task_future_timeout_on_time(spawner: impl Spawn + Clone + Send + 'static) {
        // Create a mock time service:
        let (mut tick_sender, tick_receiver) = mpsc::channel::<()>(0);
        let mut timer_client = create_timer_incoming(tick_receiver, spawner.clone()).unwrap();

        let (sender, receiver) = oneshot::channel::<()>();
        let timer_stream = timer_client.request_timer_stream().await.unwrap();
        let receiver = receiver.map(|res| res.unwrap());
        let timeout_fut = spawner
            .spawn_with_handle(future_timeout(receiver, timer_stream, 8))
            .unwrap();

        for _ in 0..7usize {
            tick_sender.send(()).await.unwrap();
        }

        sender.send(()).unwrap();
        assert_eq!(timeout_fut.await, Some(()));
    }

    #[test]
    fn test_future_timeout_on_time() {
        let thread_pool = ThreadPool::new().unwrap();
        LocalPool::new().run_until(task_future_timeout_on_time(thread_pool.clone()));
    }

    async fn task_future_timeout_late(spawner: impl Spawn + Clone + Send + 'static) {
        let (sender, receiver) = oneshot::channel::<()>();

        let (mut tick_sender, timer_stream) = mpsc::channel(0);
        let timer_stream = timer_stream.map(|_| TimerTick);
        let receiver = receiver.map(|res| res.unwrap());
        let timeout_fut = spawner
            .spawn_with_handle(future_timeout(receiver, timer_stream, 8))
            .unwrap();

        for _ in 0..9usize {
            let _ = tick_sender.send(()).await;
        }

        let _ = sender.send(());
        assert_eq!(timeout_fut.await, None);
    }

    #[test]
    fn test_future_timeout_late() {
        let thread_pool = ThreadPool::new().unwrap();
        LocalPool::new().run_until(task_future_timeout_late(thread_pool.clone()));
    }
}
