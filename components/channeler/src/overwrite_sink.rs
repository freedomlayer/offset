use std::marker::Unpin;
use core::pin::Pin;
use futures::task::{Poll, LocalWaker, Spawn, SpawnExt};
use futures::channel::mpsc;
use futures::{future, Future, FutureExt, TryFutureExt, StreamExt};

#[derive(Debug)]
enum OverwriteChannelError {
    SendError,
}

struct OverwriteChannel<T> {
    opt_item: Option<T>,
    sender: mpsc::Sender<T>,
    opt_receiver: Option<mpsc::Receiver<T>>,
}

impl<T> OverwriteChannel<T> {
    fn new(sender: mpsc::Sender<T>,
           receiver: mpsc::Receiver<T>) -> OverwriteChannel<T> {

        OverwriteChannel {
            opt_item: None,
            sender,
            opt_receiver: Some(receiver),
        }
    }
}


impl<T: Unpin> Future for OverwriteChannel<T> {
    type Output = Result<(), OverwriteChannelError>;

    fn poll(mut self: Pin<&mut Self>, lw: &LocalWaker) -> Poll<Self::Output> {
        let mut fself = Pin::new(&mut self);
        loop {
            let recv_progress = if let Some(mut receiver) = fself.opt_receiver.take() {
                match receiver.poll_next_unpin(lw) {
                    Poll::Ready(Some(item)) => {
                        // We discard the previous item and store the new one:
                        fself.opt_item = Some(item);
                        fself.opt_receiver = Some(receiver);
                        true
                    },
                    Poll::Ready(None) => {
                        // No more incoming items
                        false
                    },
                    Poll::Pending => {
                        fself.opt_receiver = Some(receiver);
                        false
                    },
                }
            } else {
                false
            };

            if let Some(item) = fself.opt_item.take() {
                match fself.sender.poll_ready(lw) {
                    Poll::Ready(Ok(())) => {
                        match fself.sender.start_send(item) {
                            Ok(()) => {},
                            Err(_) => return Poll::Ready(Err(OverwriteChannelError::SendError)),
                        }
                    },
                    Poll::Pending => {
                        fself.opt_item = Some(item);
                        if !recv_progress {
                            return Poll::Pending;
                        }
                    },
                    Poll::Ready(Err(_)) => return Poll::Ready(Err(OverwriteChannelError::SendError)),
                }
            } else {
                if fself.opt_receiver.is_none() {
                    return Poll::Ready(Ok(()));
                }
                else {
                    return Poll::Pending;
                }
            }
        }
    }
}


pub fn overwrite_channel<T,S>(mut spawner: S) -> (mpsc::Sender<T>, mpsc::Receiver<T>) 
where
    S: Spawn,
    T: Send + 'static + Unpin,
{
    let (sender, overwrite_receiver) = mpsc::channel::<T>(0);
    let (overwrite_sender, receiver) = mpsc::channel::<T>(0);

    let overwrite_fut = OverwriteChannel::new(overwrite_sender, overwrite_receiver)
        .map_err(|e| { error!("[Channeler] OverwriteChannel error: {:?}", e); })
        .then(|_| future::ready(()));

    spawner.spawn(overwrite_fut).unwrap();

    (sender, receiver)
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::{stream, StreamExt, SinkExt};
    use futures::executor::ThreadPool;
    use futures::task::{Spawn, SpawnExt};

    async fn task_overwrite_sink_send_all(spawner: impl Spawn) {
        // let (sender, mut receiver) = mpsc::channel::<u32>(0);
        // let mut overwrite_sender = OverwriteSink::new(sender);
        let (mut sender, mut receiver) = overwrite_channel::<u32,_>(spawner);

        let mut st = stream::iter(3u32 ..= 7);
        await!(sender.send_all(&mut st)).unwrap();
        drop(sender);
        let mut last_item = None;
        while let Some(item) = await!(receiver.next()) {
            last_item = Some(item);
        }
        assert_eq!(last_item, Some(7));
    }

    #[test]
    fn test_overwrite_sink_send_all() {
        let mut thread_pool = ThreadPool::new().unwrap();
        thread_pool.run(task_overwrite_sink_send_all(thread_pool.clone()));
    }

    async fn task_overwrite_sink_single_send(spawner: impl Spawn) {
        // let (sender, mut receiver) = mpsc::channel::<u32>(0);
        // let mut overwrite_sender = OverwriteSink::new(sender);
        let (mut sender, mut receiver) = overwrite_channel::<u32,_>(spawner);

        await!(sender.send(3)).unwrap();
        await!(sender.send(4)).unwrap();
        await!(sender.send(5)).unwrap();
        await!(sender.send(6)).unwrap();
        await!(sender.send(7)).unwrap();
        drop(sender);
        let mut last_item = None;
        while let Some(item) = await!(receiver.next()) {
            println!("item = {:?}", item);
            last_item = Some(item);
        }
        assert_eq!(last_item, Some(7));

    }

    #[test]
    fn test_overwrite_sink_single_send() {
        let mut thread_pool = ThreadPool::new().unwrap();
        thread_pool.run(task_overwrite_sink_single_send(thread_pool.clone()));
    }
}

// TODO: Better tests for this code?

