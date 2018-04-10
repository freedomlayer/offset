use futures::{Future, Sink};
use futures::sync::{mpsc, oneshot};

use crypto::identity::{PublicKey, Signature};

use security_module::messages::ToSecurityModule;

#[derive(Debug)]
pub enum SecurityModuleClientError {
    RequestSendFailed,
    OneshotReceiverCanceled,
}

#[derive(Clone)]
pub struct SecurityModuleClient {
    requests_sender: mpsc::Sender<ToSecurityModule>,
}

impl SecurityModuleClient {
    pub fn new(requests_sender: mpsc::Sender<ToSecurityModule>) -> Self {
        SecurityModuleClient { requests_sender }
    }

    /// Send a request to the SecurityModule. Returns a Future that waits for the response.
    fn request_response<R>(
        &self,
        request: ToSecurityModule,
        rx: oneshot::Receiver<R>,
    ) -> impl Future<Item = R, Error = SecurityModuleClientError> {
        self.requests_sender
            .clone()
            .send(request)
            .map_err(|_| SecurityModuleClientError::RequestSendFailed)
            .and_then(|_| {
                rx.map_err(|oneshot::Canceled| SecurityModuleClientError::OneshotReceiverCanceled)
            })
    }

    /// Request a signature over a provided message.
    /// Returns a Future that resolves to the calculated signature.
    pub fn request_signature(
        &self,
        message: Vec<u8>,
    ) -> impl Future<Item = Signature, Error = SecurityModuleClientError> {
        let (tx, rx) = oneshot::channel();
        let request = ToSecurityModule::RequestSignature {
            message,
            response_sender: tx,
        };
        self.request_response(request, rx)
            .and_then(|response_signature| Ok(response_signature.signature))
    }

    /// Request the public key of the used Identity.
    /// Returns a Future that resolves to the public key.
    pub fn request_public_key(
        &self,
    ) -> impl Future<Item = PublicKey, Error = SecurityModuleClientError> {
        let (tx, rx) = oneshot::channel();
        let request = ToSecurityModule::RequestPublicKey {
            response_sender: tx,
        };
        self.request_response(request, rx)
            .and_then(|response_public_key| Ok(response_public_key.public_key))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ring;
    use ring::test::rand::FixedByteRandom;
    use tokio_core::reactor::Core;

    use crypto::identity::{verify_signature, SoftwareEd25519Identity};
    use security_module::create_security_module;

    #[test]
    fn test_security_module_consistent_public_key_with_client() {
        let secure_rand = FixedByteRandom { byte: 0x3 };
        let pkcs8 = ring::signature::Ed25519KeyPair::generate_pkcs8(&secure_rand).unwrap();
        let identity = SoftwareEd25519Identity::from_pkcs8(&pkcs8).unwrap();

        let (requests_sender, sm) = create_security_module(identity);
        let smc = SecurityModuleClient::new(requests_sender);

        // Start the SecurityModule service:
        let mut core = Core::new().unwrap();
        let handle = core.handle();
        handle.spawn(sm.then(|_| Ok(())));

        let public_key1 = core.run(smc.request_public_key()).unwrap();
        let public_key2 = core.run(smc.request_public_key()).unwrap();

        assert_eq!(public_key1, public_key2);
    }

    #[test]
    fn test_security_module_request_sign_with_client() {
        let secure_rand = FixedByteRandom { byte: 0x3 };
        let pkcs8 = ring::signature::Ed25519KeyPair::generate_pkcs8(&secure_rand).unwrap();
        let identity = SoftwareEd25519Identity::from_pkcs8(&pkcs8).unwrap();

        let (requests_sender, sm) = create_security_module(identity);
        let smc = SecurityModuleClient::new(requests_sender);

        // SecurityModuleClient can be cloned:
        let smc = smc.clone();

        let my_message = b"This is my message!";

        // Start the SecurityModule service:
        let mut core = Core::new().unwrap();
        let handle = core.handle();
        handle.spawn(sm.then(|_| Ok(())));

        let public_key = core.run(smc.request_public_key()).unwrap();
        let signature = core.run(smc.request_signature(my_message.to_vec())).unwrap();

        assert!(verify_signature(&my_message[..], &public_key, &signature));
    }

    // TODO: Add tests that check "concurrency": Multiple clients that send requests.
}
