use futures::sync::mpsc;
use futures::{Future, IntoFuture};
use futures::stream::Stream;
use futures::sink::Sink;

use ::async_mutex::{AsyncMutex, AsyncMutexError};

#[derive(Debug)]
pub enum ServiceClientError {
    SendFailed,
    ReceiveFailed,
    NoResponseReceived,
    AcquireFailed,
}

struct ServiceClientInner<S, R> {
    sender: mpsc::Sender<S>,
    receiver: mpsc::Receiver<R>,
}

pub struct ServiceClient<S, R> {
    inner: AsyncMutex<ServiceClientInner<S, R>>,
}

// TODO: Why can't we #[derive(Clone)] here?
// Causes problems with cloning at SecurityModuleClient.
impl<S, R> Clone for ServiceClient<S, R> {
    fn clone(&self) -> Self {
        ServiceClient {
            inner: self.inner.clone(),
        }
    }
}

/// A service client.
/// Allows multiple futures on the same thread to send requests to a remote service.
impl<S, R> ServiceClient<S, R> {
    pub fn new(sender: mpsc::Sender<S>, receiver: mpsc::Receiver<R>) -> Self {
        ServiceClient {
            inner: AsyncMutex::new(ServiceClientInner {
                sender,
                receiver,
            }),
        }
    }

    /// Send a request of type S to service.
    /// Returns a future that resolves to a response of type R from the service.
    pub fn request(&self, request: S) -> impl Future<Item=R, Error=ServiceClientError> {
        self.inner.acquire(|inner| {
            let ServiceClientInner { sender, receiver } = inner;
            sender.send(request)
                .map_err(|_e: mpsc::SendError<S>| {
                    ServiceClientError::SendFailed
                })
                .and_then(|sender| {
                    receiver.into_future()
                        .map_err(|(_e, _receiver): ((), _)| {
                            ServiceClientError::ReceiveFailed
                        })
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
                AsyncMutexError::Function(client_resp_err) => client_resp_err,
                _ => ServiceClientError::AcquireFailed,
            }
        })
    }
}

#[cfg(test)]
mod tests {
    extern crate tokio_core;

    use super::*;

    use std::cell::RefCell;
    use std::rc::Rc;

    use self::tokio_core::reactor::Core;

    #[test]
    fn test_service_client_sequential() {
        let (sender1, receiver1) = mpsc::channel::<usize>(0);
        let (sender2, receiver2) = mpsc::channel::<usize>(0);

        // This is a service that receives a number and increases it by 1:
        let receiver2_inc = receiver2
            .map(|x| {
                x + 1
            });
        let fut_inc = sender1
            .sink_map_err(|e| ())
            .send_all(receiver2_inc);


        let mut core = Core::new().unwrap();
        let handle = core.handle();

        // Start the increaser future:
        handle.spawn(fut_inc.then(|_| Ok(())));

        let service_client = ServiceClient::new(sender2, receiver1);
        assert_eq!(core.run(service_client.request(0)).unwrap(), 1);
        assert_eq!(core.run(service_client.clone().request(6)).unwrap(), 7);
        assert_eq!(core.run(service_client.request(8)).unwrap(), 9);
    }


    #[test]
    fn test_service_client_simultaneous() {
        let (sender1, receiver1) = mpsc::channel::<usize>(0);
        let (sender2, receiver2) = mpsc::channel::<usize>(0);

        // This is a service that receives a number and increases it by 1:
        let receiver2_inc = receiver2
            .map(|x| {
                x + 1
            });
        let fut_inc = sender1
            .sink_map_err(|e| ())
            .send_all(receiver2_inc);


        let mut core = Core::new().unwrap();
        let handle = core.handle();

        let service_client = ServiceClient::new(sender2, receiver1);
        let correct_counter: Rc<RefCell<usize>> = Rc::new(RefCell::new(0));

        let c_correct_counter1 = Rc::clone(&correct_counter);
        let fut1 = service_client.request(5)
            .and_then(move |r| {
                if r == 6 {
                    *c_correct_counter1.borrow_mut() += 1;
                }
                Ok(())
            });

        // Start the increaser future:
        handle.spawn(fut_inc.then(|_| Ok(())));

        let c_correct_counter2 = Rc::clone(&correct_counter);
        let fut2 = service_client.request(8)
            .and_then(move |r| {
                if r == 9 {
                    *c_correct_counter2.borrow_mut() += 1;
                }
                Ok(())
            });

        let c_correct_counter3 = Rc::clone(&correct_counter);
        let fut3 = service_client.request(4)
            .and_then(move |r| {
                if r == 5 {
                    *c_correct_counter3.borrow_mut() += 1;
                }
                Ok(())
            });

        handle.spawn(fut2.map_err(|_| ()));
        handle.spawn(fut1.map_err(|_| ()));
        handle.spawn(fut3.map_err(|_| ()));

        assert_eq!(core.run(service_client.request(0)).unwrap(), 1);
        assert_eq!(*correct_counter.borrow(), 3);
    }
}
