#![allow(unused)]
use core::pin::Pin;
use futures::channel::oneshot;
use futures::task::Waker;
use futures::{Poll, Sink, Stream, StreamExt};
use std::marker::Unpin;

use crypto::identity::PublicKey;

/// A struct that reports when it is dropped.
struct Tracked<T> {
    inner: T,
    opt_drop_sender: Option<oneshot::Sender<()>>,
}

impl<T> Tracked<T> {
    pub fn new(inner: T, drop_sender: oneshot::Sender<()>) -> Tracked<T> {
        Tracked {
            inner,
            opt_drop_sender: Some(drop_sender),
        }
    }
}

impl<T> Stream for Tracked<T>
where
    T: Stream + Unpin,
{
    type Item = T::Item;

    fn poll_next(mut self: Pin<&mut Self>, lw: &Waker) -> Poll<Option<Self::Item>> {
        self.inner.poll_next_unpin(lw)
    }
}

impl<T> Drop for Tracked<T> {
    fn drop(&mut self) {
        match self.opt_drop_sender.take() {
            Some(drop_sender) => {
                let _ = drop_sender.send(());
            }
            None => {}
        };
    }
}

async fn conn_limiter<M, K, KE, T>(incoming_conns: T, max_conns: usize) -> Result<(), ()>
where
    T: Stream<Item = (M, K, PublicKey)>,
    M: Stream<Item = Vec<u8>>,
    K: Sink<SinkItem = Vec<u8>, SinkError = KE>,
{
    let mut cur_conns: usize = 0;
    unimplemented!();
}
