extern crate futures;

use self::futures::sync::mpsc;
use self::futures::Future;
use self::futures::stream::Stream;
use self::futures::sink::Sink;

use ::async_mutex::{AsyncMutex, AsyncMutexError};

#[derive(Debug)]
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
            println!("Acquired!");
            let ServiceClientInner { sender, receiver } = inner;
            sender.send(request)
                .map_err(|_e: mpsc::SendError<S>| ServiceClientError::SendFailed)
                .and_then(|sender| {
                    println!("Sent!");
                    receiver.into_future()
                        .map_err(|(e, receiver): ((), _)| ServiceClientError::ReceiveFailed)
                        .map(|(opt_item, receiver)| (opt_item, receiver, sender))
                }).and_then(|(opt_item, receiver, sender)| {
                    println!("Received!");
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


#[cfg(test)]
mod tests {
    extern crate tokio_core;
    use super::*;
    use self::tokio_core::reactor::Core;

    #[test]
    fn test_service_client() {
        let (sender1,receiver1) = mpsc::channel::<usize>(0);
        let (sender2,receiver2) = mpsc::channel::<usize>(0);

        let fut_inc = receiver2
            .map(|x| {
                println!("Increasing by 1!");
                x + 1
            })
            .forward(sender1.sink_map_err(|e| {
                println!("e = {:?}",e);
                // TODO: Find the problem here:
                println!("Sink error occured!");
                ()
            }));

        let mut core = Core::new().unwrap();
        let handle = core.handle();

        // Start the increaser future:
        handle.spawn(fut_inc.then(|_| Ok(())));

        let service_client = ServiceClient::new(sender2, receiver1);
        assert_eq!(core.run(service_client.request(0)).unwrap(),1);
        // assert_eq!(core.run(service_client.request(6)).unwrap(),7);
        // assert_eq!(core.run(service_client.request(8)).unwrap(),9);

        
    }
}
