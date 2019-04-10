use futures::channel::{mpsc, oneshot};
use futures::{future, stream, SinkExt, Stream, StreamExt};
use std::marker::Unpin;

#[derive(Debug)]
pub enum MultiConsumerError {}

#[derive(Debug)]
pub enum MultiConsumerClientError {
    SendError,
    ReceiveError,
}

pub struct MultiConsumerRequest<T> {
    response_sender: oneshot::Sender<mpsc::Receiver<T>>,
}

#[derive(Clone)]
pub struct MultiConsumerClient<T> {
    request_sender: mpsc::Sender<MultiConsumerRequest<T>>,
}

impl<T> MultiConsumerClient<T> {
    pub fn new(request_sender: mpsc::Sender<MultiConsumerRequest<T>>) -> Self {
        MultiConsumerClient { request_sender }
    }

    pub async fn request_stream(&mut self) -> Result<mpsc::Receiver<T>, MultiConsumerClientError> {
        // Prepare request:
        let (response_sender, response_receiver) = oneshot::channel();
        let multi_consumer_request = MultiConsumerRequest { response_sender };

        // Send request:
        await!(self.request_sender.send(multi_consumer_request))
            .map_err(|_| MultiConsumerClientError::SendError)?;

        // Wait for response:
        await!(response_receiver).map_err(|_| MultiConsumerClientError::ReceiveError)
    }
}

/// A MultiConsumer loop event
#[allow(clippy::enum_variant_names)]
enum Event<T> {
    IncomingItem(T),
    IncomingItemsClosed,
    IncomingRequest(MultiConsumerRequest<T>),
    IncomingRequestsClosed,
}

/// A service for splitting a stream into multiple streams.
/// Requires that the sent item is Clone.
/// Should be used together with a MultiConsumerClient to request new streams.
pub async fn multi_consumer_service<T, I>(
    incoming_items: I,
    incoming_requests: mpsc::Receiver<MultiConsumerRequest<T>>,
) -> Result<(), MultiConsumerError>
where
    T: Clone,
    // TODO: Can we avoid the Unpin requirement here?
    I: Stream<Item = T> + Unpin,
{
    let incoming_items = incoming_items
        .map(Event::IncomingItem)
        .chain(stream::once(future::ready(Event::IncomingItemsClosed)));

    let incoming_requests = incoming_requests
        .map(Event::IncomingRequest)
        .chain(stream::once(future::ready(Event::IncomingRequestsClosed)));

    let mut incoming = incoming_items.select(incoming_requests);
    let mut incoming_requests_closed = false;
    let mut senders: Vec<mpsc::Sender<T>> = Vec::new();

    while let Some(event) = await!(incoming.next()) {
        match event {
            Event::IncomingItem(t) => {
                let mut new_senders = Vec::new();
                for mut sender in senders {
                    if await!(sender.send(t.clone())).is_ok() {
                        new_senders.push(sender);
                    }
                }
                senders = new_senders;
                if senders.is_empty() && incoming_requests_closed {
                    // There are no more clients, and new clients can not register.
                    // We exit.
                    return Ok(());
                }
            }
            Event::IncomingItemsClosed => break,
            Event::IncomingRequest(request) => {
                let (sender, receiver) = mpsc::channel(0);
                if request.response_sender.send(receiver).is_ok() {
                    senders.push(sender);
                }
            }
            Event::IncomingRequestsClosed => incoming_requests_closed = true,
        }
    }
    Ok(())
}

// TODO: Add tests
