extern crate futures;

use self::futures::sync::mpsc;
use self::futures::Future;
use self::futures::stream::Stream;
use self::futures::sink::Sink;

use ::async_mutex::{AsyncMutex, AsyncMutexError};

pub enum ServiceClientError {
    SendFailed,
    ReceiveFailed,
    NoResponseReceived,
    AcquireFailed,
}

struct ServiceClientInner<S,R> {
    sender: mpsc::Sender<S>, 
    receiver: mpsc::Receiver<R>,
}

pub struct ServiceClient<S,R> {
    inner: AsyncMutex<ServiceClientInner<S,R>>,
}

impl<S,R> ServiceClient<S,R> {
    pub fn new(sender: mpsc::Sender<S>, receiver: mpsc::Receiver<R>) -> Self {
        ServiceClient {
            inner: AsyncMutex::new(ServiceClientInner {
                sender,
                receiver,
            }),
        }
    }

    pub fn request(&self, request: S) -> impl Future<Item=R, Error=ServiceClientError> {
        self.inner.acquire(|inner| {
            let ServiceClientInner { sender, receiver } = inner;
            sender.send(request)
                .map_err(|_e: mpsc::SendError<S>| ServiceClientError::SendFailed)
                .and_then(|sender| {
                    receiver.into_future()
                        .map_err(|(e, receiver): ((), _)| ServiceClientError::ReceiveFailed)
                        .map(|(opt_item, receiver)| (opt_item, receiver, sender))
                }).and_then(|(opt_item, receiver, sender)| {
                    let item = match opt_item {
                        Some(item) => item,
                        None => return Err(ServiceClientError::NoResponseReceived),
                    };
                    Ok((ServiceClientInner { sender, receiver }, item))
                })
        }).map_err(|e| {
            match e {
                AsyncMutexError::FuncError(client_response_error) => client_response_error,
                _ => ServiceClientError::AcquireFailed,
            }
        })
    }
}

