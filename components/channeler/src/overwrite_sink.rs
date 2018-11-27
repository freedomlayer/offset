use std::marker::Unpin;
use core::pin::Pin;
use futures::task::{Poll, LocalWaker};
use futures::Sink;


struct OverwriteSink<S,T> {
    sink: S,
    opt_item: Option<T>,
}

impl<S,T,E> OverwriteSink<S,T> 
where
    S: Sink<SinkItem=T, SinkError=E> + Unpin,
    T: Unpin,
{
    fn new(sink: S) -> OverwriteSink<S,T> {
        OverwriteSink { sink, opt_item: None }
    }
}

impl<S,T,E> Sink for OverwriteSink<S,T> 
where
    S: Sink<SinkItem=T, SinkError=E> + Unpin,
    T: Unpin,
{
    type SinkItem = T;
    type SinkError = E;

    fn poll_ready(self: Pin<&mut Self>, _lw: &LocalWaker) -> Poll<Result<(), E>> {
        Poll::Ready(Ok(()))
    }

    fn start_send(mut self: Pin<&mut Self>, item: T) -> Result<(), E> {
        println!("start_send");
        self.opt_item = Some(item);
        Ok(())
    }

    fn poll_flush(mut self: Pin<&mut Self>, lw: &LocalWaker
    ) -> Poll<Result<(), E>> {
        println!("poll_flush");
        let item = if let Some(item) = self.opt_item.take() {
            item
        } else {
            return Poll::Ready(Ok(()));
        };

        match Pin::new(&mut self.sink).poll_ready(lw) {
            Poll::Pending => {
                self.opt_item = Some(item);
                return Poll::Pending;
            },
            Poll::Ready(Ok(())) => Pin::new(&mut self.sink).start_send(item)?,
            Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
        };
        Pin::new(&mut self.sink).poll_flush(lw)
    }

    fn poll_close(mut self: Pin<&mut Self>, lw: &LocalWaker) -> Poll<Result<(), E>> {
        println!("poll_close");
        match Pin::new(&mut self).poll_flush(lw) {
            Poll::Ready(Ok(())) => {},
            Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
            Poll::Pending => return Poll::Pending,
        };
        Pin::new(&mut self.sink).poll_close(lw)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::{stream, StreamExt, SinkExt};
    use futures::executor::ThreadPool;
    use futures::task::{Spawn, SpawnExt};
    use futures::channel::mpsc;

    async fn task_overwrite_sink_send_all() {
        let (sender, mut receiver) = mpsc::channel::<u32>(0);
        let mut overwrite_sender = OverwriteSink::new(sender);

        let mut st = stream::iter(3u32 ..= 5);
        await!(overwrite_sender.send_all(&mut st)).unwrap();
        assert_eq!(await!(receiver.next()), Some(5));
        let mut st = stream::iter(6u32 ..= 7);
        await!(overwrite_sender.send_all(&mut st)).unwrap();
        assert_eq!(await!(receiver.next()), Some(7));
        drop(overwrite_sender);
        assert_eq!(await!(receiver.next()), None);
    }

    #[test]
    fn test_overwrite_sink_send_all() {
        let mut thread_pool = ThreadPool::new().unwrap();
        thread_pool.run(task_overwrite_sink_send_all());
    }

    async fn task_overwrite_sink_single_send() {
        let (sender, mut receiver) = mpsc::channel::<u32>(0);
        let mut overwrite_sender = OverwriteSink::new(sender);

        await!(overwrite_sender.send(3)).unwrap();
        await!(overwrite_sender.send(4)).unwrap();
        await!(overwrite_sender.send(5)).unwrap();
        assert_eq!(await!(receiver.next()), Some(5));
        await!(overwrite_sender.send(6)).unwrap();
        await!(overwrite_sender.send(7)).unwrap();
        assert_eq!(await!(receiver.next()), Some(7));
        drop(overwrite_sender);
        assert_eq!(await!(receiver.next()), None);
    }

    #[test]
    fn test_overwrite_sink_single_send() {
        let mut thread_pool = ThreadPool::new().unwrap();
        thread_pool.run(task_overwrite_sink_single_send());
    }
}


// ===================================== 

/*
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

    spawner.spawn(overwrite_fut);

    (sender, receiver)
}


*/

// TODO: 
// - Implement this code as a Stream instead of a spawned future. 
// We should be the receiver. Is this possible/reasonable?
//
// - How to test this code?
