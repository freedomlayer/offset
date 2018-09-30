use futures::prelude::{async, await};
use futures::Stream;

#[derive(Debug, Eq, PartialEq)]
pub enum ReceiveError {
    Closed,
    Error,
}

/// Util function to read one item from a Stream, asynchronously.
#[async]
pub fn receive<T, EM, M: 'static>(reader: M) -> Result<(T, M), ReceiveError>
    where M: Stream<Item=T, Error=EM>,
{
    match await!(reader.into_future()) {
        Ok((opt_reader_message, ret_reader)) => {
            match opt_reader_message {
                Some(reader_message) => Ok((reader_message, ret_reader)),
                None => return Err(ReceiveError::Closed),
            }
        },
        Err(_) => return Err(ReceiveError::Error),
    }
}
