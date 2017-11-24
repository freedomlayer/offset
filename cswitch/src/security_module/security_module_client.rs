extern crate futures;

use std::collections::VecDeque;
use std::mem;

use self::futures::sync::mpsc;
use self::futures::sync::oneshot;
use self::futures::Future;
use self::futures::stream::Stream;
use self::futures::sink::Sink;

enum ServiceClientError {
    SendFailed,
    ReceiveFailed,
    NoResponseReceived,
}

struct InnerService<S,R> {
    sender: mpsc::Sender<S>, 
    receiver: mpsc::Receiver<R>,
}

enum ServiceClientState<S,R> {
    Available(InnerService<S,R>),
    Busy(VecDeque<InnerService<S,R>>),
    Empty,
}

struct ServiceClient<S,R> {
    state: ServiceClientState<S,R>,
}

fn request_future<S,R>(request: S, inner_service: InnerService<S,R>) -> impl Future<Item=(R, InnerService), Error=ServiceClientError> {
    let InnerService { sender, receiver } = inner_service;
    sender.send(request)
        .map_err(|e: mpsc::SendError| ServiceClientError::SendFailed)
        .and_then(|sender| {
            receiver.into_future()
                .map_err(|(e, receiver): ((), _)| ServiceClientError::ReceiveFailed)
                .and_then(|opt_item, receiver| {
                    match opt_item {
                        Some(response) => {
                            Ok(response)
                        },
                        None => Err(ServiceClientError::NoResponseReceived),
                    }
                })
        })
}

impl<S,R> ServiceClient<S,R> {
    fn request(&mut self, request: S) -> impl Future<Item=R, Error=ServiceClientError> {
        match mem::replace(&mut self.state, ServiceClientState::Empty) {
            ServiceClientState::Empty => unreachable!(),
            ServiceClientState::Available(InnerService {sender, receiver}) => {
                // request_future(request, inner_service)
            },
            ServiceClientState::Busy(pending) => {

            },
        }
        /*
        self.sender.send(s)
            .map_err(|e: mpsc::SendError| ServiceClientError::SendFailed)
            .and_then(|sender| 
        */

    }
}


