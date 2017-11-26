extern crate futures;
extern crate futures_mutex;

use std::ops::DerefMut;
use std::mem;

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
    impl Future<Item=(R, InnerService<S,R>), Error=ServiceClientError> {

    let InnerService { sender, receiver } = inner_service;
    sender.send(request)
        .map_err(|_e: mpsc::SendError<S>| ServiceClientError::SendFailed)
        .and_then(|sender| {
            receiver.into_future()
                .map_err(|(e, receiver): ((), _)| ServiceClientError::ReceiveFailed)
                .and_then(|(opt_item, receiver)| {
                    match opt_item {
                        Some(response) => {
                            Ok((response, InnerService {sender, receiver}))
                        },
                        None => Err(ServiceClientError::NoResponseReceived),
                    }
                })
        })
}

/*
impl<S,R> ServiceClient<S,R> {
    fn request(&mut self, request: S) -> impl Future<Item=R, Error=ServiceClientError> {
        let state_lock = match mem::replace(&mut self.state_lock_opt, None) {
            None => unreachable!(),
            Some(state_lock) => state_lock,
        };
        state_lock.lock()
            .map_err(|()| unreachable!())
            .and_then(|acquired| {
                let inner_service = match mem::replace(acquired.deref_mut(), None) {
                    None => unreachable!(),
                    Some(inner_service) => inner_service,
                };

                let new_inner_service = request_future(request, inner_service);
                mem::swap(acquired.deref_mut(), &mut new_inner_service);

                // TODO: Fix ownership problem here somehow:
                let inner_service = acquired.deref_mut().unwrap();
                request_future(request, inner_service)
            })
    }
        /*
        self.sender.send(s)
            .map_err(|e: mpsc::SendError| ServiceClientError::SendFailed)
            .and_then(|sender| 
        */

}

*/
