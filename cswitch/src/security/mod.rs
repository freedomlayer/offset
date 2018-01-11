use std::mem;

use futures::prelude::*;
use futures::sync::{mpsc, oneshot};

use utils::CloseHandle;
use utils::crypto::identity::Identity;

pub mod client;
pub mod messages;

use self::client::SecurityModuleClient;
use self::messages::{FromSecurityModule, ToSecurityModule};

// TODO: Possibly Make this structure of service future more generic.  
// Separate process_request from the rest of the code, 
// so that we could construct more such services.

enum SecurityModuleState {
    Reading(usize),
    PendingSend {
        dest_index: usize,
        value: FromSecurityModule,
    },
    Closing,
    Closed,
}

pub struct SecurityModule<I> {
    identity: I,
    close_receiver: oneshot::Receiver<()>,
    close_sender_opt: Option<oneshot::Sender<()>>,
    client_endpoints: Vec<(mpsc::Sender<FromSecurityModule>, mpsc::Receiver<ToSecurityModule>)>,
    state: SecurityModuleState,
}

/// Create a new security module, together with a close handle to be used after the security module
/// future instance was consumed.
pub fn create_security_module<I: Identity>(identity: I) -> (CloseHandle, SecurityModule<I>) {
    let (close_handle, (close_sender, close_receiver)) = CloseHandle::new();
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
    /// A client may be used by multiple futures in the same thread.
    pub fn new_client(&mut self) -> SecurityModuleClient {
        let (sm_sender, client_receiver) = mpsc::channel(0);
        let (client_sender, sm_receiver) = mpsc::channel(0);

        self.client_endpoints.push((sm_sender, sm_receiver));
        SecurityModuleClient::new(client_sender, client_receiver)
    }

    /// Process a request, and produce a response.
    fn process_request(&self, request: ToSecurityModule) -> FromSecurityModule {
        match request {
            ToSecurityModule::RequestSign {message} => {
                FromSecurityModule::ResponseSign {
                    signature: self.identity.sign_message(&message),
                }
            },
            ToSecurityModule::RequestPublicKey {} => {
                FromSecurityModule::ResponsePublicKey {
                    public_key: self.identity.get_public_key(),
                }
            },
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
            Err(_e) => Err(SecurityModuleError::CloseReceiverCanceled),
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

pub enum SecurityModuleError {
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
                        self.state = SecurityModuleState::Reading(j);
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
                            if first_unready_index.take().is_none() {
                                first_unready_index = Some(j);
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
    use super::*;
    // use utils::crypto::uid::gen_uid;
    use utils::crypto::identity::{SoftwareEd25519Identity, verify_signature};

    use ring;
    use rand::{Rng, StdRng};
    use tokio_core::reactor::Core;
    use ring::test::rand::FixedByteRandom;

    #[test]
    fn test_security_module_consistent_public_key() {
        let secure_rand = FixedByteRandom { byte: 0x3 };
        let pkcs8 = ring::signature::Ed25519KeyPair::generate_pkcs8(&secure_rand).unwrap();
        let identity = SoftwareEd25519Identity::from_pkcs8(&pkcs8).unwrap();
        
        let (sm_handle, mut sm) = create_security_module(identity);
        let sm_client = sm.new_client();

        // Start the SecurityModule service:
        let mut core = Core::new().unwrap();
        let handle = core.handle();
        handle.spawn(sm.then(|_| Ok(())));

        let public_key1 = core.run(sm_client.clone().request_public_key()).unwrap();
        let public_key2 = core.run(sm_client.request_public_key()).unwrap();
        assert_eq!(public_key1, public_key2);
    }

    #[test]
    fn test_security_module_request_sign() {
        let secure_rand = FixedByteRandom { byte: 0x3 };
        let pkcs8 = ring::signature::Ed25519KeyPair::generate_pkcs8(&secure_rand).unwrap();
        let identity = SoftwareEd25519Identity::from_pkcs8(&pkcs8).unwrap();
        
        let (sm_handle, mut sm) = create_security_module(identity);
        let sm_client = sm.new_client();

        let my_message = b"This is my message!";

        // Start the SecurityModule service:
        let mut core = Core::new().unwrap();
        let handle = core.handle();
        handle.spawn(sm.then(|_| Ok(())));

        let public_key = core.run(sm_client.request_public_key()).unwrap();
        let signature = core.run(sm_client.clone().request_sign(my_message.to_vec())).unwrap();

        assert!(verify_signature(&my_message[..], &public_key, &signature));
    }

    // TODO: Add tests that check "concurrency": Multiple clients that send requests.
}
