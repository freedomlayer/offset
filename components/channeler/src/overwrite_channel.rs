use std::marker::Unpin;
use core::pin::Pin;
use futures::channel::mpsc;
use futures::task::{Spawn, SpawnExt, Poll, LocalWaker};
use futures::{future, Future, FutureExt, select, TryFuture, TryFutureExt, StreamExt, SinkExt};

#[derive(Debug)]
pub enum OverwriteChannelError {
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

    spawner.spawn(overwrite_fut);

    (sender, receiver)
}

// TODO: 
// - Implement this code as a Stream instead of a spawned future. 
// We should be the receiver. Is this possible/reasonable?
//
// - How to test this code?
