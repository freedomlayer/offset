extern crate futures;

use std::mem;

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

enum SecurityModuleState {
    Empty,
    Reading(usize),
    PendingSend {
        dest_index: usize,
        value: FromSecurityModule,
    },
    Closing,
}


struct SecurityModule<I> {
    identity: I,
    close_receiver: oneshot::Receiver<()>,
    client_endpoints: Vec<(mpsc::Sender<FromSecurityModule>, mpsc::Receiver<ToSecurityModule>)>,
    state: SecurityModuleState,
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
            state: SecurityModuleState::Reading(0),
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

    /// Try to close all the senders
    fn attempt_close(&mut self) -> Poll<(), SecurityModuleError> {
        // If we are in closing state, work on closing all client endpoints
        // We try to close all the senders.
        while let Some((mut sender, receiver)) = self.client_endpoints.pop() {
            match sender.close() {
                Ok(Async::Ready(())) => {},
                Ok(Async::NotReady) => {
                    self.client_endpoints.push((sender, receiver));
                    return Ok(Async::NotReady);
                },
                Err(_e) => return Err(SecurityModuleError::ErrorClosingSender),
            }
        }
        Ok(Async::Ready(()))
    }

    /// Check if we need to close
    fn check_should_close(&mut self) -> Result<bool, SecurityModuleError> {
        match self.close_receiver.poll() {
            Ok(Async::Ready(())) => {
                // We were asked to close
                Ok(true)
            },
            Ok(Async::NotReady) => Ok(false),
            Err(_e) => return Err(SecurityModuleError::CloseReceiverCanceled),
        }
    }

}

enum SecurityModuleError {
    SenderError(mpsc::SendError<FromSecurityModule>),
    CloseReceiverCanceled,
    ErrorClosingSender,
}

impl<I: Identity> Future for SecurityModule<I> {
    type Item = ();
    type Error = SecurityModuleError;


    /// The main loop for SecurityModule operation.
    /// Receives requests from clients and returns responses.
    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        loop {
            // If there is something waiting to be sent, try to send it first:
            match mem::replace(&mut self.state, SecurityModuleState::Empty) {
                SecurityModuleState::Empty => {
                    panic!("Invalid state");
                }
                SecurityModuleState::Closing => {
                    self.state = SecurityModuleState::Closing;
                    return self.attempt_close()
                },
                SecurityModuleState::PendingSend { dest_index, value } => {
                    let &mut (ref mut sender, _) = &mut self.client_endpoints[dest_index];
                    match sender.start_send(value) {
                        Ok(AsyncSink::Ready) => {},
                        Ok(AsyncSink::NotReady(value)) => {
                            self.state = SecurityModuleState::PendingSend {dest_index, value};
                            return Ok(Async::NotReady)
                        }
                        Err(e) => {return Err(SecurityModuleError::SenderError(e))},
                    }
                }
                SecurityModuleState::Reading(j) => {
                    if self.check_should_close()? {
                        self.state = SecurityModuleState::Closing;
                        continue
                    }


                    // TODO: Try to read from remaining receivers in this round:
                    for i in j .. self.client_endpoints.len() {
                        let &mut (_, ref mut receiver) = &mut self.client_endpoints[i];
                        // TODO: receiver.poll
                    }

                }
            }
        }
        // We get here if there is nothing more to be sent and we are not in a closing state.

        // Check if we need to close:
    

        // Wait on all the receivers, to see if there is a message in any of them.
        // In addition, check if close receiver has a message.
        



        
        // - Calculate responses for each of the requests.
        
        // - Send back the responses to the correct requesters.

        Ok(Async::Ready(()))
    }

}
