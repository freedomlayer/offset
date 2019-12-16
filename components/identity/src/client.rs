use futures::channel::{mpsc, oneshot};
use futures::{Future, TryFutureExt};

use common::futures_compat::send_to_sink;
use proto::crypto::{PublicKey, Signature};

use crate::messages::{ResponsePublicKey, ResponseSignature, ToIdentity};

#[derive(Debug)]
pub enum IdentityClientError {
    RequestSendFailed,
    OneshotReceiverCanceled,
}

#[derive(Debug, Clone)]
pub struct IdentityClient {
    requests_sender: mpsc::Sender<ToIdentity>,
}

impl IdentityClient {
    pub fn new(requests_sender: mpsc::Sender<ToIdentity>) -> Self {
        IdentityClient { requests_sender }
    }

    /// Send a request to the Identity. Returns a Future that waits for the response.
    fn request_response<R>(
        &self,
        request: ToIdentity,
        rx: oneshot::Receiver<R>,
    ) -> impl Future<Output = Result<R, IdentityClientError>> {
        send_to_sink(self.requests_sender.clone(), request)
            .map_err(|_| IdentityClientError::RequestSendFailed)
            .and_then(|_| {
                rx.map_err(|oneshot::Canceled| IdentityClientError::OneshotReceiverCanceled)
            })
    }

    /// Request a signature over a provided message.
    /// Returns a Future that resolves to the calculated signature.
    pub fn request_signature(
        &self,
        message: Vec<u8>,
    ) -> impl Future<Output = Result<Signature, IdentityClientError>> {
        let (tx, rx) = oneshot::channel::<ResponseSignature>();
        let request = ToIdentity::RequestSignature {
            message,
            response_sender: tx,
        };
        self.request_response(request, rx)
            .map_ok(|response_signature| response_signature.signature)
    }

    /// Request the public key of the used Identity.
    /// Returns a Future that resolves to the public key.
    pub fn request_public_key(
        &self,
    ) -> impl Future<Output = Result<PublicKey, IdentityClientError>> {
        let (tx, rx) = oneshot::channel();
        let request = ToIdentity::RequestPublicKey {
            response_sender: tx,
        };
        self.request_response(request, rx)
            .map_ok(|response_public_key: ResponsePublicKey| response_public_key.public_key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use futures::executor::LocalPool;
    use futures::future;
    use futures::task::SpawnExt;
    use futures::FutureExt;

    use crate::identity::create_identity;
    use crypto::identity::{verify_signature, SoftwareEd25519Identity};
    use crypto::rand::RandGen;
    use crypto::test_utils::DummyRandom;

    use proto::crypto::PrivateKey;

    #[test]
    fn test_identity_consistent_public_key_with_client() {
        let secure_rand = DummyRandom::new(&[3u8]);
        let private_key = PrivateKey::rand_gen(&secure_rand);
        let identity = SoftwareEd25519Identity::from_private_key(&private_key).unwrap();

        let (requests_sender, sm) = create_identity(identity);
        let smc = IdentityClient::new(requests_sender);

        // Start the Identity service:
        let mut local_pool = LocalPool::new();
        let spawner = local_pool.spawner();
        spawner.spawn(sm.then(|_| future::ready(()))).unwrap();

        let public_key1 = local_pool.run_until(smc.request_public_key()).unwrap();
        let public_key2 = local_pool.run_until(smc.request_public_key()).unwrap();

        assert_eq!(public_key1, public_key2);
    }

    #[test]
    fn test_identity_request_sign_with_client() {
        let secure_rand = DummyRandom::new(&[3u8]);
        let private_key = PrivateKey::rand_gen(&secure_rand);
        let identity = SoftwareEd25519Identity::from_private_key(&private_key).unwrap();

        let (requests_sender, sm) = create_identity(identity);
        let smc = IdentityClient::new(requests_sender);

        // IdentityClient can be cloned:
        let smc = smc.clone();

        let my_message = b"This is my message!";

        // Start the Identity service:
        let mut local_pool = LocalPool::new();
        let spawner = local_pool.spawner();
        spawner.spawn(sm.then(|_| future::ready(()))).unwrap();

        let public_key = local_pool.run_until(smc.request_public_key()).unwrap();
        let signature = local_pool
            .run_until(smc.request_signature(my_message.to_vec()))
            .unwrap();

        assert!(verify_signature(&my_message[..], &public_key, &signature));
    }

    // TODO: Add tests that check "concurrency": Multiple clients that send requests.
}
