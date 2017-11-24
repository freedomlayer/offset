extern crate futures;
extern crate futures_mutex;

use std::ops::DerefMut;

use self::futures::sync::mpsc;
use self::futures::sync::oneshot;
use self::futures::Future;
use self::futures::stream::Stream;
use self::futures::sink::Sink;

use self::futures_mutex::FutMutex;

enum ServiceClientError {
    SendFailed,
    ReceiveFailed,
    NoResponseReceived,
}

struct InnerService<S,R> {
    sender: mpsc::Sender<S>, 
    receiver: mpsc::Receiver<R>,
}

struct ServiceClient<S,R> {
    state_lock_opt: Option<FutMutex<Option<InnerService<S,R>>>>,
}

fn request_future<S,R>(request: S, inner_service: InnerService<S,R>) -> 
    impl Future<Item=R, Error=ServiceClientError> {

    let InnerService { sender, receiver } = inner_service;
    sender.send(request)
        .map_err(|_e: mpsc::SendError<S>| ServiceClientError::SendFailed)
        .and_then(|sender| {
            receiver.into_future()
                .map_err(|(e, receiver): ((), _)| ServiceClientError::ReceiveFailed)
                .and_then(|(opt_item, receiver)| {
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
        match self.state_lock_opt.take() {
            None => unreachable!(),
            Some(state_lock) => {
                state_lock.lock()
                .map_err(|()| unreachable!())
                .and_then(|mut acquired| {
                    // TODO: Fix ownership problem here somehow:
                    let inner_service = acquired.deref_mut().unwrap();
                    request_future(request, inner_service)
                })
            },
        }
        /*
        self.sender.send(s)
            .map_err(|e: mpsc::SendError| ServiceClientError::SendFailed)
            .and_then(|sender| 
        */

    }
}


