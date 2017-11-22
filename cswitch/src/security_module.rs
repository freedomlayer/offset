extern crate futures;

use std::mem;

use self::futures::{Future, Stream, Poll, Async, AsyncSink};
use self::futures::future::{ok, loop_fn};
use self::futures::sink::Sink;
use self::futures::sync::mpsc;
use self::futures::sync::oneshot;


use ::inner_messages::{ToSecurityModule, FromSecurityModule};
use ::identity::{Identity};
use ::close_handle::{CloseHandle, create_close_handle};

// TODO: Possibly Make this structure of service future more generic.  
// Separate process_request from the rest of the code, 
// so that we could construct more such services.

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
    Closing,
    Closed,
}


struct SecurityModule<I> {
    identity: I,
    close_receiver: oneshot::Receiver<()>,
    close_sender_opt: Option<oneshot::Sender<()>>,
    client_endpoints: Vec<(mpsc::Sender<FromSecurityModule>, mpsc::Receiver<ToSecurityModule>)>,
    state: SecurityModuleState,
}

/// Create a new security module, together with a close handle to be used after the security module
/// future instance was consumed.
fn create_security_module<I: Identity>(identity: I) -> (CloseHandle, SecurityModule<I>) {
    let (close_handle, (close_sender, close_receiver)) = create_close_handle();
    let security_module = SecurityModule::new(identity, close_sender, close_receiver);
    (close_handle, security_module)
}

impl<I: Identity> SecurityModule<I> {
    /// Create a security module together with a security module handle.
    /// The handle can be used to notify the security module to close, remotely.
    fn new(identity: I, close_sender: oneshot::Sender<()>, close_receiver: oneshot::Receiver<()>) -> Self {
        SecurityModule {
            identity,
            close_sender_opt: Some(close_sender),
            close_receiver,
            client_endpoints: Vec::new(),
            state: SecurityModuleState::Reading(0),
        }
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
            },
            /*
            ToSecurityModule::RequestSymmetricKey { request_id,
                                                    public_key,
                                                    salt} => {
                FromSecurityModule::ResponseSymmetricKey {
                    request_id,
                    symmetric_key: self.identity.gen_symmetric_key(
                        &public_key, &salt),
                }
            },
            */
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
        let close_sender = match self.close_sender_opt.take() {
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
    RemoteClientClosed,
}

impl<I: Identity> Future for SecurityModule<I> {
    type Item = ();
    type Error = SecurityModuleError;

    // TODO: Possibly implement poll() using loop_fn():

    /// The main loop for SecurityModule operation.
    /// Receives requests from clients and returns responses.
    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        let mut first_unready_index: Option<usize> = None;
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
                SecurityModuleState::PendingSend { dest_index, value } => {
                    let start_send_res = {
                        let &mut (ref mut sender, _) = &mut self.client_endpoints[dest_index];
                        sender.start_send(value)
                    };
                    match start_send_res {
                        Ok(AsyncSink::Ready) => {
                            let next_index = (dest_index + 1) % self.client_endpoints.len();
                            self.state = SecurityModuleState::Reading(next_index);
                        },
                        Ok(AsyncSink::NotReady(value)) => {
                            self.state = SecurityModuleState::PendingSend {dest_index, value};
                            return Ok(Async::NotReady)
                        }
                        Err(e) => return Err(SecurityModuleError::SenderError(e)),
                    }
                }
                SecurityModuleState::Reading(j) => {
                    if first_unready_index == Some(j) {
                        // We made a full cycle but no receiver was ready.
                        return Ok(Async::NotReady);
                    }

                    if self.check_should_close()? {
                        self.state = SecurityModuleState::Closing;
                        continue
                    }

                    let receiver_poll_res = {
                        let &mut (_, ref mut receiver) = &mut self.client_endpoints[j];
                        receiver.poll()
                    };

                    match receiver_poll_res {
                        Ok(Async::Ready(Some(request))) => {
                            self.state = SecurityModuleState::PendingSend {
                                dest_index: j,
                                value: self.process_request(request),
                            };
                            continue;
                        },
                        Ok(Async::Ready(None)) => {
                            return Err(SecurityModuleError::RemoteClientClosed);
                        },
                        Ok(Async::NotReady) => {
                            // Track the first unready index:
                            match first_unready_index.take() {
                                None => first_unready_index = Some(j),
                                _ => {},
                            };
                            let next_j = (j + 1) % self.client_endpoints.len();
                            self.state = SecurityModuleState::Reading(next_j);
                            continue;
                        },
                        Err(()) => return Err(SecurityModuleError::ErrorReceivingRequest),
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    extern crate tokio_core;
    extern crate rand;
    extern crate ring;

    use super::*;
    use ::uid::UidGenerator;
    use ::identity::SoftwareEd25519Identity;
    // use ::test_utils::DummyRandom;

    use self::rand::{Rng, StdRng};
    use self::tokio_core::reactor::Core;


    #[test]
    fn test_security_module_request_public_key() {

        /*
        let secure_rand = DummyRandom::new(&[1,2,3,4,5]);
        let pkcs8 = ring::signature::Ed25519KeyPair::generate_pkcs8(&secure_rand).unwrap();
        let identity = SoftwareEd25519Identity::from_pkcs8(&pkcs8).unwrap();
        */

        let rng_seed: &[_] = &[1,2,3,4,5,6];
        let mut rng: StdRng = rand::SeedableRng::from_seed(rng_seed);
        let mut identity_seed = [0; 32];
        rng.fill_bytes(&mut identity_seed);
        let identity = SoftwareEd25519Identity::new(&identity_seed);

        
        let (sm_handle, mut sm) = create_security_module(identity);
        let (client_sender, client_receiver) = sm.new_client();

        let rng_seed: &[_] = &[1,2,3,4,5];
        let rng: StdRng = rand::SeedableRng::from_seed(rng_seed);
        let mut uid_gen = UidGenerator::new(rng);

        let request_id0 = uid_gen.gen_uid();
        let request_id1 = uid_gen.gen_uid();


        let fut_public_key = client_sender
            .send(ToSecurityModule::RequestPublicKey { request_id: request_id0.clone() })
            .map_err(|_| panic!("Send error!"))
            .and_then(|client_sender| {
                client_receiver
                .into_future()
                .and_then(|(opt, client_receiver)| {
                    let public_key = match opt {
                        None => panic!("Receiver was closed!"),
                        Some(FromSecurityModule::ResponsePublicKey 
                             { request_id, public_key }) => {
                            assert_eq!(request_id, request_id0);
                            public_key
                        },
                        Some(_) => panic!("Invalid response was received!"),
                    };
                    Ok((client_sender, client_receiver, public_key))
                })
                .map_err(|(_e, _client_sender)| panic!("Reading error!"))
            });


        let mut core = Core::new().unwrap();
        let handle = core.handle();

        handle.spawn(sm.then(|_| Ok(())));
        let (client_sender, client_receiver, public_key) = 
            core.run(fut_public_key).unwrap();
    }

    // TODO: Add tests that check "concurrency": Multiple clients that send requests.
}
