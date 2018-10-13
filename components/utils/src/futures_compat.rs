use std::thread;
use std::time::{Duration, Instant};
use futures::sink::{SinkExt};
use futures::channel::mpsc;
use tokio::prelude::{Sink as TokioSink, Stream as TokioStream, Future as TokioFuture};
use tokio::timer::Interval;

fn interval_thread(duration: Duration, sender: mpsc::Sender<()>) {
    let interval = Interval::new(Instant::now(), duration)
        .map_err(|_| ());
    let my_tokio_fut = interval.for_each(move |_| {
        let c_sender = sender.clone().compat();
        c_sender.send(())
            .map_err(|_| ())
            .map(|_| ())
    });
    tokio::run(my_tokio_fut);
}


/// Returns a stream that receives an empty () messages every constant amount of time.
pub fn create_interval(duration: Duration) -> mpsc::Receiver<()> {
    let (sender, receiver) = mpsc::channel::<()>(0);
    thread::spawn(move || interval_thread(duration, sender));
    receiver
}


#[cfg(test)]
mod tests {
    use super::*;
    use futures::stream::StreamExt;
    use futures::executor::LocalPool;

    #[test]
    fn test_create_interval() {
        let my_interval = create_interval(Duration::from_millis(10));
        let my_fut = my_interval.take(10).collect::<Vec<()>>();

        let mut local_pool = LocalPool::new();
        local_pool.run_until(my_fut);
    }
}
