use futures::{Stream, StreamExt};

#[derive(Debug, Eq, PartialEq)]
pub enum ReceiveError {
    Closed,
    Error,
}

/// Util function to read one item from a Stream, asynchronously.
pub async fn receive<T, M: 'static>(reader: M) -> Option<(T, M)>
    where M: Stream<Item=T> + std::marker::Unpin,
{
    match await!(reader.into_future()) {
        (opt_reader_message, ret_reader) => {
            match opt_reader_message {
                Some(reader_message) => Some((reader_message, ret_reader)),
                None => None
            }
        },
    }
}
