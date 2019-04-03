use futures::{Stream, StreamExt};
use std::marker::Unpin;

#[derive(Debug, Eq, PartialEq)]
pub enum ReceiveError {
    Closed,
    Error,
}

/// Util function to read one item from a Stream, asynchronously.
pub async fn receive<T, M: 'static>(mut reader: M) -> Option<(T, M)>
where
    M: Stream<Item = T> + Unpin,
{
    match await!(reader.next()) {
        Some(reader_msg) => Some((reader_msg, reader)),
        None => None,
    }
}
