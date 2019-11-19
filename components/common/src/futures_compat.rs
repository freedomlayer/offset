use futures::sink::{Sink, SinkExt};
use futures::{stream, Future, FutureExt, StreamExt};
use std::marker::Unpin;

// TODO; Is there a better way to do this with the new Futures libraries?
/// A helper function to allow sending into a sink while consuming the sink.
/// Futures 3.0's Sink::send function takes a mutable reference to the sink instead of consuming
/// it.
/// See: https://users.rust-lang.org/t/discarding-unused-cloned-sender-futures-3-0/21228
pub async fn send_to_sink<S, T, E>(mut sink: S, item: T) -> Result<S, E>
where
    S: Sink<T, Error = E> + std::marker::Unpin + 'static,
{
    sink.send(item).await?;
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
