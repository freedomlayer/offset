use std::pin::Pin;
use std::collections::VecDeque;
use futures::{Stream, StreamExt, Poll};
use futures::task::LocalWaker;

pub type BoxStream<'a, T> = Pin<Box<dyn Stream<Item=T> + Send + 'a>>;

struct StreamKeeper<'a, T> {
    stream: BoxStream<'a, T>,
    polled: bool,
}

impl<'a,T> StreamKeeper<'a,T> {
    fn new(stream: BoxStream<'a,T>) -> Self {
        StreamKeeper {
            stream,
            polled: false,
        }
    }
}

pub struct SelectStreams<'a,T> {
    stream_keepers: VecDeque<StreamKeeper<'a,T>>
}

impl<'a,T> Stream for SelectStreams<'a,T> {
    type Item = T;

    fn poll_next(mut self: Pin<&mut Self>, waker: &LocalWaker) -> Poll<Option<Self::Item>> {

        for sk in &mut self.stream_keepers {
            sk.polled = false;
        }

        while let Some(mut stream_keeper) = self.stream_keepers.pop_front() {

            // Make sure we don't poll the same stream twice in one cycle:
            if stream_keeper.polled {
                return Poll::Pending;
            }
            stream_keeper.polled = true;

            match stream_keeper.stream.poll_next_unpin(waker) {
                Poll::Pending => {
                    self.stream_keepers.push_back(stream_keeper);
                },
                Poll::Ready(Some(t)) => {
                    self.stream_keepers.push_back(stream_keeper);
                    return Poll::Ready(Some(t));
                },
                Poll::Ready(None) => {}
            }
        }
        // No more streams to poll:
        return Poll::Ready(None);
    }
}

pub fn select_streams<'a,T>(streams: Vec<BoxStream<'a,T>>) -> SelectStreams<'a,T> {
    SelectStreams { 
        stream_keepers: streams.into_iter().map(StreamKeeper::new).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::stream;
    use futures::executor::block_on;

    #[test]
    fn test_select_stream_basic() {
        let s1 = stream::iter(vec![1,2,3,4u8]);
        let s2 = stream::iter(vec![5,6,7,8,9u8]);
        let s3 = stream::iter(vec![10,11,12,13u8]);

        let streams: Vec<BoxStream<'static, u8>> = vec![Box::pin(s1), Box::pin(s2), Box::pin(s3)];
        let selected = select_streams(streams);

        let result = block_on(selected.collect::<Vec<u8>>());
        assert_eq!(result.len(), 4 + 5 + 4);
    }

    // TODO: Add more tests here.
}
