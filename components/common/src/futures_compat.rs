use futures::channel::mpsc;
use futures::sink::{Sink, SinkExt};
use futures::{stream, Future, FutureExt, StreamExt};
use std::marker::Unpin;
use std::thread;
use std::time::{Duration, Instant};
use tokio::prelude::{Future as TokioFuture, Sink as TokioSink, Stream as TokioStream};
use tokio::timer::Interval;

fn interval_thread(duration: Duration, sender: mpsc::Sender<()>) {
    let interval = Interval::new(Instant::now(), duration).map_err(|_| ());
    let my_tokio_fut = interval.for_each(move |_| {
        let c_sender = sender.clone().compat();
        c_sender.send(()).map_err(|_| ()).map(|_| ())
    });
    tokio::run(my_tokio_fut);
}

/// Returns a stream that receives an empty () messages every constant amount of time.
pub fn create_interval(duration: Duration) -> mpsc::Receiver<()> {
    let (sender, receiver) = mpsc::channel::<()>(0);
    thread::spawn(move || interval_thread(duration, sender));
    receiver
}

/// A helper function to allow sending into a sink while consuming the sink.
/// Futures 3.0's Sink::send function takes a mutable reference to the sink instead of consuming
/// it.
/// See: https://users.rust-lang.org/t/discarding-unused-cloned-sender-futures-3-0/21228
pub async fn send_to_sink<S, T, E>(mut sink: S, item: T) -> Result<S, E>
where
    S: Sink<T, SinkError = E> + std::marker::Unpin + 'static,
{
    await!(sink.send(item))?;
    Ok(sink)
}

/// A futures select function that outputs an Unpin future.
pub fn future_select<T>(
    a: impl Future<Output = T> + Unpin,
    b: impl Future<Output = T> + Unpin,
) -> impl Future<Output = T> + Unpin {
    let s_a = stream::once(a);
    let s_b = stream::once(b);
    let s = stream::select(s_a, s_b);
    s.into_future().map(|(opt_item, _s)| opt_item.unwrap())
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::executor::LocalPool;
    use futures::stream::StreamExt;

    #[test]
    fn test_create_interval() {
        let my_interval = create_interval(Duration::from_millis(10));
        let my_fut = my_interval.take(10).collect::<Vec<()>>();

        let mut local_pool = LocalPool::new();
        local_pool.run_until(my_fut);
    }
}
