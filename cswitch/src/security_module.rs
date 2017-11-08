extern crate futures;

use self::futures::{Future, Poll, Async, AsyncSink};
use self::futures::sink::Sink;
use self::futures::sync::mpsc;
use self::futures::sync::oneshot;


use ::inner_messages::{ToSecurityModule, FromSecurityModule};
use ::identity::{Identity};

struct SecurityModuleHandle {
    close_sender: oneshot::Sender<()>,
}

/// This handle is used to send a closing notification to SecurityModuleHandle,
/// even after it was consumed.
impl SecurityModuleHandle {
    /// Send a close message to SecurityModuleHandle
    fn close(self) {
        self.close_sender.send(());
    }
}

struct PendingSend {
    dest_index: usize,
    value: FromSecurityModule,
}


struct SecurityModule<I> {
    identity: I,
    close_receiver: oneshot::Receiver<()>,
    client_endpoints: Vec<(mpsc::Sender<FromSecurityModule>, mpsc::Receiver<ToSecurityModule>)>,
    pending_send: Option<PendingSend>,
}

impl<I: Identity> SecurityModule<I> {
    /// Create a security module together with a security module handle.
    /// The handle can be used to notify the security module to close, remotely.
    fn create(identity: I) -> (SecurityModuleHandle, Self) {

        // Create a oneshot channel between the handle and the security module:
        let (sender, receiver) = oneshot::channel();

        let sm_handle = SecurityModuleHandle {
            close_sender: sender,
        };

        let sm = SecurityModule {
            identity,
            client_endpoints: Vec::new(),
            close_receiver: receiver,
            pending_send: None,
        };

        (sm_handle, sm)
    }


    /// Create a new client for the security module.
    /// Remote side is given a sender side of a channel, and a receiver side of a channel.
    fn new_client(&mut self) -> (mpsc::Sender<ToSecurityModule>, 
                                 mpsc::Receiver<FromSecurityModule>) {
       let (sm_sender, client_receiver) = mpsc::channel(0);
       let (client_sender, sm_receiver) = mpsc::channel(0);

       self.client_endpoints.push((sm_sender, sm_receiver));
       (client_sender, client_receiver)

    }
}

enum SecurityModuleError {
    SenderError(mpsc::SendError<FromSecurityModule>),
}

impl<I: Identity> Future for SecurityModule<I> {
    type Item = ();
    type Error = SecurityModuleError;

    /// The main loop for SecurityModule operation.
    /// Receives requests from clients and returns responses.
    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {

        
        // If there is something waiting to be sent, try to send it first:
        self.pending_send = match self.pending_send.take() {
            None => None,
            Some(PendingSend {dest_index, value}) => {
                let &mut (ref mut sender, _) = &mut self.client_endpoints[dest_index];
                match sender.start_send(value) {
                    Ok(AsyncSink::Ready) => None,
                    Ok(AsyncSink::NotReady(value)) => Some(PendingSend {dest_index, value}),
                    Err(e) => {return Err(SecurityModuleError::SenderError(e))},
                }
            }
        };
        
        // TODO: Need to exit in case of something waiting to be sent.
        //

        // Wait on all the receivers, to see if there is a message in any of them.
        // In addition, check if close receiver has a message.
        

        for &(_, ref receiver) in &self.client_endpoints {
            // receiver.poll
        }

        
        // - Calculate responses for each of the requests.
        
        // - Send back the responses to the correct requesters.

        Ok(Async::Ready(()))
    }

}
