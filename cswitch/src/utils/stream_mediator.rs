use std::mem;
use futures::prelude::*;
use futures::sync::mpsc;

use tokio_core::reactor::Handle;

const INTERNAL_CHANNEL_CAPACITY: usize = 512;

struct StreamMediatorFuture<S: Stream + 'static>
    where S::Item: Clone
{
    upstream: S,
    incoming: mpsc::Receiver<mpsc::Sender<S::Item>>,
    clients: Vec<Option<mpsc::Sender<S::Item>>>,
}

pub enum StreamMediatorError<E> {
    InternalError,
    StreamError(E),
}

impl<S: Stream + 'static> StreamMediatorFuture<S>
    where S::Item: Clone
{
    fn broadcast(&mut self, msg: S::Item) {
        for client in &mut self.clients {
            let mut sender = client.take().expect("encounter a dropped client");
            match sender.start_send(msg.clone()) {
                Err(_e) => {
                    info!("client disconnected, client will be removed");
                }
                Ok(start_send) => {
                    match start_send {
                        AsyncSink::Ready => {
                            // For now, this should always succeed
                            if sender.poll_complete().is_ok() {
                                mem::replace(client, Some(sender));
                            }
                        }
                        AsyncSink::NotReady(_) => {
                            warn!("failed to send item, client will be removed");
                        }
                    }
                }
            }
        }

        // Remove the disconnected clients
        self.clients.retain(|client| client.is_some());
    }
}

impl<S: Stream + 'static> Future for StreamMediatorFuture<S>
    where S::Item: Clone
{
    type Item = ();
    type Error = StreamMediatorError<S::Error>;

    fn poll(&mut self) -> Poll<(), Self::Error> {
        loop {
            let poll_incoming = self.incoming.poll()
                .map_err(|_| StreamMediatorError::InternalError);

            match poll_incoming? {
                Async::Ready(None) => {
                    return Ok(Async::Ready(()));
                }
                Async::Ready(Some(tx)) => {
                    self.clients.push(Some(tx));
                }
                Async::NotReady => {
                    let poll_upstream = self.upstream.poll()
                        .map_err(StreamMediatorError::StreamError);

                    match try_ready!(poll_upstream) {
                        None => {
                            self.incoming.close();
                            return Ok(Async::Ready(()));
                        }
                        Some(item) => {
                            // FIXME: If the upstream buffered messages > client sender
                            // buffer capacity, the client will be drop before wake up.
                            self.broadcast(item);
                        }
                    }
                }
            }
        }
    }
}

pub struct StreamMediator<S: Stream + 'static> {
    inner: mpsc::Sender<mpsc::Sender<S::Item>>,
}

impl<S: Stream + 'static> StreamMediator<S>
    where S::Item: Clone
{
    pub fn new(stream: S, handle: &Handle) -> StreamMediator<S> {
        let (inner_tx, inner_rx) = mpsc::channel(INTERNAL_CHANNEL_CAPACITY);

        let mediator_task = StreamMediatorFuture {
            upstream: stream,
            clients: Vec::new(),
            incoming: inner_rx,
        };

        handle.spawn(
            mediator_task.map_err(|_| {
                warn!("stream mediator exited");
            })
        );

        StreamMediator { inner: inner_tx }
    }

    pub fn get_stream(&mut self) -> Result<mpsc::Receiver<S::Item>, ()> {
        let (sender, receiver) = mpsc::channel(INTERNAL_CHANNEL_CAPACITY);

        self.inner.try_send(sender).map_err(|_| ())?;

        Ok(receiver)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::stream::iter_ok;
    use futures::future::join_all;

    use tokio_core::reactor::Core;

    const CLIENT_NUM: usize = 100;

    #[test]
    fn broadcast() {
        let v = vec![0, 1, 2, 3, 4];

        let mut core = Core::new().unwrap();
        let handle = core.handle();

        let mut mediator = StreamMediator::new(iter_ok::<_, ()>(v), &handle);

        let client_tasks = (0..CLIENT_NUM).map(|x| {
            let stream = mediator.get_stream().unwrap();
            stream.collect().and_then(|result| {
                assert_eq!(result, vec![0, 1, 2, 3, 4]);
                Ok(())
            })
        }).collect::<Vec<_>>();

        core.run(join_all(client_tasks)).unwrap();
    }
}