use core::pin::Pin;
use std::marker::Unpin;

use futures::task::{Context, Poll};
use futures::{Future, Sink, Stream, StreamExt};

struct OverwriteChannel<T, M, K> {
    opt_item: Option<T>,
    sender: K,
    opt_receiver: Option<M>,
}

impl<T, M, K> OverwriteChannel<T, M, K> {
    fn new(sender: K, receiver: M) -> OverwriteChannel<T, M, K> {
        OverwriteChannel {
            opt_item: None,
            sender,
            opt_receiver: Some(receiver),
        }
    }
}

impl<T, M, K> Future for OverwriteChannel<T, M, K>
where
    T: Unpin,
    M: Stream<Item = T> + Unpin,
    K: Sink<T> + Unpin,
{
    type Output = Result<(), K::Error>;

    fn poll(mut self: Pin<&mut Self>, context: &mut Context) -> Poll<Self::Output> {
        let mut fself = Pin::new(&mut self);
        loop {
            let recv_progress = if let Some(mut receiver) = fself.opt_receiver.take() {
                match receiver.poll_next_unpin(context) {
                    Poll::Ready(Some(item)) => {
                        // We discard the previous item and store the new one:
                        fself.opt_item = Some(item);
                        fself.opt_receiver = Some(receiver);
                        true
                    }
                    Poll::Ready(None) => {
                        // No more incoming items
                        false
                    }
                    Poll::Pending => {
                        fself.opt_receiver = Some(receiver);
                        false
                    }
                }
            } else {
                false
            };

            if let Some(item) = fself.opt_item.take() {
                match Pin::new(&mut fself.sender).poll_ready(context) {
                    Poll::Ready(Ok(())) => match Pin::new(&mut fself.sender).start_send(item) {
                        Ok(()) => {}
                        Err(e) => return Poll::Ready(Err(e)),
                    },
                    Poll::Pending => {
                        fself.opt_item = Some(item);
                        if !recv_progress {
                            return Poll::Pending;
                        }
                    }
                    Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
                }
            } else if fself.opt_receiver.is_none() {
                return Poll::Ready(Ok(()));
            } else {
                return Poll::Pending;
            }
        }
    }
}

/// Attempt to send all messages from coming from the receiver stream through the sender sink.
/// If a message is pending to be sent and a new message arrives, it overwrites the old message.
/// For example: a sequence 1,2,3,4,5,6,7 may be received as 1,2,5,7
pub fn overwrite_send_all<T, E, M, K>(sender: K, receiver: M) -> impl Future<Output = Result<(), E>>
where
    T: Unpin,
    M: Stream<Item = T> + Unpin,
    K: Sink<T, Error = E> + Unpin,
{
    OverwriteChannel::new(sender, receiver)
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::channel::mpsc;
    use futures::executor::{ThreadPool, block_on};
    use futures::task::{Spawn, SpawnExt};
    use futures::{stream, SinkExt, StreamExt};
    use futures::{FutureExt, TryFutureExt};

    fn overwrite_channel<T, S>(mut spawner: S) -> (mpsc::Sender<T>, mpsc::Receiver<T>)
    where
        S: Spawn,
        T: Send + 'static + Unpin,
    {
        let (sender, overwrite_receiver) = mpsc::channel::<T>(0);
        let (overwrite_sender, receiver) = mpsc::channel::<T>(0);

        let overwrite_fut = overwrite_send_all(overwrite_sender, overwrite_receiver)
            .map_err(|e| {
                error!("[Channeler] OverwriteChannel error: {:?}", e);
            })
            .map(|_| ());

        spawner.spawn(overwrite_fut).unwrap();

        (sender, receiver)
    }

    async fn task_overwrite_sink_send_all(spawner: impl Spawn) {
        // let (sender, mut receiver) = mpsc::channel::<u32>(0);
        // let mut overwrite_sender = OverwriteSink::new(sender);
        let (mut sender, mut receiver) = overwrite_channel::<u32, _>(spawner);

        let mut st = stream::iter(3u32..=7);
        sender.send_all(&mut st.map(Ok)).await.unwrap();
        drop(sender);
        let mut last_item = None;
        while let Some(item) = receiver.next().await {
            last_item = Some(item);
        }
        assert_eq!(last_item, Some(7));
    }

    #[test]
    fn test_overwrite_sink_send_all() {
        let mut thread_pool = ThreadPool::new().unwrap();
        block_on(task_overwrite_sink_send_all(thread_pool.clone()));
    }

    async fn task_overwrite_sink_single_send(spawner: impl Spawn) {
        // let (sender, mut receiver) = mpsc::channel::<u32>(0);
        // let mut overwrite_sender = OverwriteSink::new(sender);
        let (mut sender, mut receiver) = overwrite_channel::<u32, _>(spawner);

        sender.send(3).await.unwrap();
        sender.send(4).await.unwrap();
        sender.send(5).await.unwrap();
        sender.send(6).await.unwrap();
        sender.send(7).await.unwrap();
        drop(sender);
        let mut last_item = None;
        while let Some(item) = receiver.next().await {
            last_item = Some(item);
        }
        assert_eq!(last_item, Some(7));
    }

    #[test]
    fn test_overwrite_sink_single_send() {
        let mut thread_pool = ThreadPool::new().unwrap();
        block_on(task_overwrite_sink_single_send(thread_pool.clone()));
    }
}

// TODO: Better tests for this code?
