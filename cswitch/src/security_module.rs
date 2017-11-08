extern crate futures;

use std::mem;

use self::futures::{Future, Stream, Poll, Async, AsyncSink};
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

}

enum SecurityModuleError {
    SenderError(mpsc::SendError<FromSecurityModule>),
    CloseReceiverCanceled,
    ErrorClosingSender,
    ErrorReceivingRequest,
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
