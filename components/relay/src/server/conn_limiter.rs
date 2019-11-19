#![allow(unused)]
use core::pin::Pin;
use futures::channel::oneshot;
use futures::task::{Context, Poll};
use futures::{Sink, Stream, StreamExt};
use std::marker::Unpin;

use proto::crypto::PublicKey;

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

    fn poll_next(mut self: Pin<&mut Self>, context: &mut Context) -> Poll<Option<Self::Item>> {
        self.inner.poll_next_unpin(context)
    }
}

impl<T> Drop for Tracked<T> {
    fn drop(&mut self) {
        if let Some(drop_sender) = self.opt_drop_sender.take() {
            let _ = drop_sender.send(());
        };
    }
}

async fn conn_limiter<M, K, KE, T>(incoming_conns: T, max_conns: usize) -> Result<(), ()>
where
    T: Stream<Item = (M, K, PublicKey)>,
    M: Stream<Item = Vec<u8>>,
    K: Sink<Vec<u8>, Error = KE>,
{
    let mut cur_conns: usize = 0;
    unimplemented!();
}
