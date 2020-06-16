use futures::channel::mpsc;
use futures::prelude::*;

use crypto::identity::Identity;

use super::messages::{ResponsePublicKey, ResponseSignature, ToIdentity};

/*
pub enum IdentityError {
    ErrorReceivingRequest,
}
*/

/// Create a new security module, together with a close handle to be used after the security module
/// future instance was consumed.
pub fn create_identity<I: Identity>(
    identity: I,
) -> (mpsc::Sender<ToIdentity>, impl Future<Output = ()>) {
    let (requests_sender, requests_receiver) = mpsc::channel::<ToIdentity>(0);
    let identity = requests_receiver.for_each(move |request| {
        match request {
            ToIdentity::RequestSignature {
                message,
                response_sender,
            } => {
                let _ = response_sender.send(ResponseSignature {
                    signature: identity.sign(&message),
                });
                // It is possible that sending the response didn't work.
                // We don't care about this.
                future::ready(())
            }
            ToIdentity::RequestPublicKey { response_sender } => {
                let _ = response_sender.send(ResponsePublicKey {
                    public_key: identity.get_public_key(),
                });
                future::ready(())
            }
        }
    });

    (requests_sender, identity)
}

#[cfg(test)]
mod tests {
    use super::*;

    use crypto::identity::{verify_signature, SoftwareEd25519Identity};
    use crypto::rand::RandGen;
    use crypto::test_utils::DummyRandom;

    use proto::crypto::PrivateKey;

    use futures::channel::oneshot;
    use futures::executor::LocalPool;
    use futures::task::SpawnExt;

    #[test]
    fn test_identity_consistent_public_key() {
        let mut rng = DummyRandom::new(&[3u8]);
        let private_key = PrivateKey::rand_gen(&mut rng);
        let identity = SoftwareEd25519Identity::from_private_key(&private_key).unwrap();
        let actual_public_key = identity.get_public_key();
        let (requests_sender, sm) = create_identity(identity);

        // Start the Identity service:
        let mut local_pool = LocalPool::new();
        let spawner = local_pool.spawner();
        spawner.spawn(sm.then(|_| future::ready(()))).unwrap();

        // Query the security module twice to check for consistency
        for _ in 0..2 {
            let mut rsender = requests_sender.clone();
            let (tx, rx) = oneshot::channel::<ResponsePublicKey>();
            let public_key_from_client = local_pool
                .run_until(
                    rsender
                        .send(ToIdentity::RequestPublicKey {
                            response_sender: tx,
                        })
                        .then(|result| match result {
                            Ok(_) => rx,
                            Err(_e) => panic!("Failed to send public key request (1) !"),
                        }),
                )
                .unwrap()
                .public_key;

            assert_eq!(actual_public_key, public_key_from_client);
        }
    }

    #[test]
    fn test_identity_request_signature_against_identity() {
        let mut rng = DummyRandom::new(&[3u8]);
        let private_key = PrivateKey::rand_gen(&mut rng);
        let identity = SoftwareEd25519Identity::from_private_key(&private_key).unwrap();
        // Get the public key straight from the Identity
        let public_key = identity.get_public_key();

        // Start the Identity service:
        let (requests_sender, sm) = create_identity(identity);
        let mut local_pool = LocalPool::new();
        let spawner = local_pool.spawner();
        spawner.spawn(sm.then(|_| future::ready(()))).unwrap();

        // Get a signature from the service
        let my_message = b"This is my message!";

        let mut rsender = requests_sender.clone();
        let (tx, rx) = oneshot::channel::<ResponseSignature>();
        let signature = local_pool
            .run_until(
                rsender
                    .send(ToIdentity::RequestSignature {
                        message: my_message.to_vec(),
                        response_sender: tx,
                    })
                    .then(|result| match result {
                        Ok(_) => rx,
                        Err(_e) => panic!("Failed to send signature request"),
                    }),
            )
            .unwrap()
            .signature;

        assert!(verify_signature(&my_message[..], &public_key, &signature));
    }

    #[test]
    fn test_identity_request_signature() {
        let mut rng = DummyRandom::new(&[3u8]);
        let private_key = PrivateKey::rand_gen(&mut rng);
        let identity = SoftwareEd25519Identity::from_private_key(&private_key).unwrap();

        // Start the Identity service:
        let (requests_sender, sm) = create_identity(identity);
        let mut local_pool = LocalPool::new();
        let spawner = local_pool.spawner();
        spawner.spawn(sm.then(|_| future::ready(()))).unwrap();

        // Get the public key from the service
        let my_message = b"This is my message!";
        let mut rsender1 = requests_sender.clone();
        let (tx1, rx1) = oneshot::channel::<ResponsePublicKey>();
        let public_key_from_client = local_pool
            .run_until(
                rsender1
                    .send(ToIdentity::RequestPublicKey {
                        response_sender: tx1,
                    })
                    .then(|result| match result {
                        Ok(_) => rx1,
                        Err(_e) => panic!("Failed to send public key request (1) !"),
                    }),
            )
            .unwrap()
            .public_key;

        // Get a signature from the service
        let mut rsender2 = requests_sender.clone();
        let (tx2, rx2) = oneshot::channel::<ResponseSignature>();
        let signature = local_pool
            .run_until(
                rsender2
                    .send(ToIdentity::RequestSignature {
                        message: my_message.to_vec(),
                        response_sender: tx2,
                    })
                    .then(|result| match result {
                        Ok(_) => rx2,
                        Err(_e) => panic!("Failed to send signature request"),
                    }),
            )
            .unwrap()
            .signature;

        assert!(verify_signature(
            &my_message[..],
            &public_key_from_client,
            &signature
        ));
    }

    // TODO: Add tests that check "concurrency": Multiple clients that send requests.
}
