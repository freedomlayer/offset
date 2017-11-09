extern crate futures;

use std::mem;

use self::futures::{Future, Stream, Poll, Async, AsyncSink};
use self::futures::sink::Sink;
use self::futures::sync::mpsc;
use self::futures::sync::oneshot;


use ::inner_messages::{ToSecurityModule, FromSecurityModule};
use ::identity::{Identity};

// TODO: Possibly Make this structure of service future more generic.  
// Separate process_request from the rest of the code, 
// so that we could construct more such services.

struct SecurityModuleHandle {
    handle_close_sender: oneshot::Sender<()>,       // Signal to close
    handle_close_receiver: oneshot::Receiver<()>,   // Closing is complete
}


/// This handle is used to send a closing notification to SecurityModuleHandle,
/// even after it was consumed.
impl SecurityModuleHandle {
    /// Send a close message to SecurityModuleHandle
    /// Returns a future that resolves when the closing is complete.
    fn close(self) -> Result<oneshot::Receiver<()>,()> {
        match self.handle_close_sender.send(()) {
            Ok(()) => Ok(self.handle_close_receiver),
            Err(_) => Err(())
        }
    }

}

struct PendingSend {
    dest_index: usize,
    value: FromSecurityModule,
}

enum SecurityModuleState {
    Reading(usize),
    PendingSend {
        dest_index: usize,
        value: FromSecurityModule,
    },
    RemoveClient(usize),
    Closing,
    Closed,
}


struct SecurityModule<I> {
    identity: I,
    close_receiver: oneshot::Receiver<()>,
    close_sender: Option<oneshot::Sender<()>>,
    client_endpoints: Vec<(mpsc::Sender<FromSecurityModule>, mpsc::Receiver<ToSecurityModule>)>,
    state: SecurityModuleState,
}


impl<I: Identity> SecurityModule<I> {
    /// Create a security module together with a security module handle.
    /// The handle can be used to notify the security module to close, remotely.
    fn create(identity: I) -> (SecurityModuleHandle, Self) {

        // Create a oneshot channel between the handle and the security module:
        let (handle_sender, sm_receiver) = oneshot::channel();
        let (sm_sender, handle_receiver) = oneshot::channel();

        let sm_handle = SecurityModuleHandle {
            handle_close_sender: handle_sender,
            handle_close_receiver: handle_receiver,
        };

        let sm = SecurityModule {
            identity,
            close_sender: Some(sm_sender),
            close_receiver: sm_receiver,
            client_endpoints: Vec::new(),
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

    /// Process a request, and produce a response.
    fn process_request(&self, request: ToSecurityModule) -> FromSecurityModule {
        match request {
            ToSecurityModule::RequestSign {request_id, message} => {
                FromSecurityModule::ResponseSign {
                    request_id,
                    signature: self.identity.sign_message(&message),
                }
            },
            ToSecurityModule::RequestVerify {request_id, 
                                            message, 
                                            public_key, 
                                            signature} => {
                FromSecurityModule::ResponseVerify {
                    request_id,
                    result: self.identity.verify_signature(
                        &message, &public_key, &signature),
                }
            },
            ToSecurityModule::RequestPublicKey { request_id } => {
                FromSecurityModule::ResponsePublicKey {
                    request_id,
                    public_key: self.identity.get_public_key(),
                }
            }
        }
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

    /// Tell the handle that we are done closing.
    fn send_close_notification(&mut self) -> Result<(), SecurityModuleError> {
        let close_sender = match self.close_sender.take() {
            None => panic!("Close notification already sent!"),
            Some(close_sender) => close_sender,
        };

        match close_sender.send(()) {
            Ok(()) => Ok(()),
            Err(_) => Err(SecurityModuleError::SendCloseNotificationFailed),
        }
    }

}

enum SecurityModuleError {
    SenderError(mpsc::SendError<FromSecurityModule>),
    CloseReceiverCanceled,
    ErrorClosingSender,
    ErrorReceivingRequest,
    SendCloseNotificationFailed,
}

impl<I: Identity> Future for SecurityModule<I> {
    type Item = ();
    type Error = SecurityModuleError;


    /// The main loop for SecurityModule operation.
    /// Receives requests from clients and returns responses.
    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        loop {
            // If there is something waiting to be sent, try to send it first:
            match mem::replace(&mut self.state, SecurityModuleState::Closed) {
                SecurityModuleState::Closed => {
                    panic!("Invalid state");
                }
                SecurityModuleState::Closing => {
                    match self.attempt_close()? {
                        Async::Ready(()) => {
                            self.state = SecurityModuleState::Closed;
                            self.send_close_notification()?;
                            return Ok(Async::Ready(()));
                        },
                        Async::NotReady => {
                            self.state = SecurityModuleState::Closing;
                            return Ok(Async::NotReady);
                        },
                    }
                },
                SecurityModuleState::RemoveClient(i) => {
                    let close_result = { 
                        let &mut (ref mut sender, _) = &mut self.client_endpoints[i];
                        sender.close()
                    };
                    match close_result {
                        Ok(Async::Ready(())) => {
                            self.client_endpoints.remove(i);
                            if self.client_endpoints.len() == 0 {
                                self.state = SecurityModuleState::Closed;
                                self.send_close_notification();
                                return Ok(Async::Ready(()));
                            } else {
                                self.state = SecurityModuleState::Reading(
                                    i % self.client_endpoints.len());
                                continue;
                            }
                        },
                        Ok(Async::NotReady) => {
                            self.state = SecurityModuleState::RemoveClient(i);
                            return Ok(Async::NotReady);
                        },
                        Err(_e) => return Err(SecurityModuleError::ErrorClosingSender),
                    }
                },
                SecurityModuleState::PendingSend { dest_index, value } => {
                    let &mut (ref mut sender, _) = &mut self.client_endpoints[dest_index];
                    match sender.start_send(value) {
                        Ok(AsyncSink::Ready) => {},
                        Ok(AsyncSink::NotReady(value)) => {
                            self.state = SecurityModuleState::PendingSend {dest_index, value};
                            return Ok(Async::NotReady)
                        }
                        Err(e) => return Err(SecurityModuleError::SenderError(e)),
                    }
                }
                SecurityModuleState::Reading(j) => {
                    if self.check_should_close()? {
                        self.state = SecurityModuleState::Closing;
                        continue
                    }

                    for i in j .. self.client_endpoints.len() {
                        let receiver_poll_res = {
                            let &mut (_, ref mut receiver) = &mut self.client_endpoints[i];
                            receiver.poll()
                        };
                        match receiver_poll_res {
                            Ok(Async::Ready(Some(request))) => {
                                let response = self.process_request(request);
                                self.state = SecurityModuleState::PendingSend {
                                    dest_index: i,
                                    value: response,
                                };
                            },
                            Ok(Async::Ready(None)) => {
                                // Remote side has closed the channel. We remove it.
                                self.state = SecurityModuleState::RemoveClient(i);
                                continue;
                            },
                            Ok(Async::NotReady) => {
                                self.state = SecurityModuleState::Reading(i);
                                return Ok(Async::NotReady)
                            },
                            Err(()) => return Err(SecurityModuleError::ErrorReceivingRequest),
                        }
                    }
                    // We are done with the reading round. We go back to the beginning.
                    self.state = SecurityModuleState::Reading(0);
                    continue;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    /*
    extern crate tokio_core;

    use self::tokio_core::reactor::Core;

    #[test]
    fn test_security_module_create() {
        let mut core = Core::new().unwrap();
        let handle = core.handle();

        let (sm_handle, sm) = SecurityModule::create();

        handle.spawn(sm.then(|_| Ok(())));

        core.run(collector).unwrap();


    }
    */

}
