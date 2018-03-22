pub mod messages;
pub mod client;

use futures::prelude::*;
use futures::sync::mpsc;

use crypto::identity::Identity;

use self::messages::{ToSecurityModule, ResponseSignature, ResponsePublicKey};

pub enum SecurityModuleError {
    ErrorReceivingRequest,
}

/// Create a new security module, together with a close handle to be used after the security module
/// future instance was consumed.
pub fn create_security_module<I: Identity>(identity: I) -> (mpsc::Sender<ToSecurityModule>, impl Future<Item=(), Error=SecurityModuleError>) {
    let (requests_sender, requests_receiver) = mpsc::channel(0);
    let security_module = requests_receiver
        .map_err(|()| SecurityModuleError::ErrorReceivingRequest)
        .for_each(move |request| {
        match request {
            ToSecurityModule::RequestSignature {message, response_sender} => {
                let _ = response_sender.send(ResponseSignature {
                    signature: identity.sign_message(&message),
                }); 
                // It is possible that sending the response didn't work.
                // We don't care about this.
                Ok(())
            },
            ToSecurityModule::RequestPublicKey {response_sender} => {
                let _ = response_sender.send(ResponsePublicKey {
                    public_key: identity.get_public_key(),
                });
                Ok(())
            },
        }
    });
    
    (requests_sender, security_module)
}


#[cfg(test)]
mod tests {
    use super::*;

    use futures::sync::oneshot;
    use ring;
    use tokio_core::reactor::Core;
    use ring::test::rand::FixedByteRandom;

    use crypto::identity::{verify_signature, SoftwareEd25519Identity};

    #[test]
    fn test_security_module_consistent_public_key() {
        let secure_rand = FixedByteRandom { byte: 0x3 };
        let pkcs8 = ring::signature::Ed25519KeyPair::generate_pkcs8(&secure_rand).unwrap();
        let identity = SoftwareEd25519Identity::from_pkcs8(&pkcs8).unwrap();
        let actual_public_key = identity.get_public_key();
        let (requests_sender, sm) = create_security_module(identity);

        // Start the SecurityModule service:
        let mut core = Core::new().unwrap();
        let handle = core.handle();
        handle.spawn(sm.then(|_| Ok(())));

        // Query the security module twice to check for consistency
        for i in 0..2 {
            let rsender = requests_sender.clone();
            let (tx, rx) = oneshot::channel();
            let public_key_from_client = core.run(rsender
                .send(ToSecurityModule::RequestPublicKey { response_sender: tx })
                .then(|result| {
                    match result {
                        Ok(_) => rx,
                        Err(_) => panic!("Failed to send public key request (1) !"),
                    }
                })).unwrap().public_key;

            assert_eq!(actual_public_key, public_key_from_client);
        }
    }

    #[test]
    fn test_security_module_request_signature_against_identity() {
        let secure_rand = FixedByteRandom { byte: 0x3 };
        let pkcs8 = ring::signature::Ed25519KeyPair::generate_pkcs8(&secure_rand).unwrap();
        let identity = SoftwareEd25519Identity::from_pkcs8(&pkcs8).unwrap();
        // Get the public key straight from the Identity
        let public_key = identity.get_public_key();

        // Start the SecurityModule service:
        let (requests_sender, sm) = create_security_module(identity);
        let mut core = Core::new().unwrap();
        let handle = core.handle();
        handle.spawn(sm.then(|_| Ok(())));

        // Get a signature from the service
        let my_message = b"This is my message!";

        let rsender = requests_sender.clone();
        let (tx, rx) = oneshot::channel();
        let signature = core.run(rsender
                 .send(ToSecurityModule::RequestSignature {message: my_message.to_vec(), response_sender: tx})
                 .then(|result| {
                     match result {
                         Ok(_) => rx,
                         Err(_) => panic!("Failed to send signature request"),
                     }
                 })).unwrap().signature;

        assert!(verify_signature(&my_message[..], &public_key, &signature));
    }

    #[test]
    fn test_security_module_request_signature() {
        let secure_rand = FixedByteRandom { byte: 0x3 };
        let pkcs8 = ring::signature::Ed25519KeyPair::generate_pkcs8(&secure_rand).unwrap();
        let identity = SoftwareEd25519Identity::from_pkcs8(&pkcs8).unwrap();



        // Start the SecurityModule service:
        let (requests_sender, sm) = create_security_module(identity);
        let mut core = Core::new().unwrap();
        let handle = core.handle();
        handle.spawn(sm.then(|_| Ok(())));

        // Get the public key from the service
        let my_message = b"This is my message!";
        let rsender1 = requests_sender.clone();
        let (tx1, rx1) = oneshot::channel();
        let public_key_from_client = core.run(rsender1
            .send(ToSecurityModule::RequestPublicKey { response_sender: tx1 })
            .then(|result| {
                match result {
                    Ok(_) => rx1,
                    Err(_) => panic!("Failed to send public key request (1) !"),
                }
            })).unwrap().public_key;

        // Get a signature from the service
        let rsender2 = requests_sender.clone();
        let (tx2, rx2) = oneshot::channel();
        let signature = core.run(rsender2
                 .send(ToSecurityModule::RequestSignature {message: my_message.to_vec(), response_sender: tx2})
                 .then(|result| {
                     match result {
                         Ok(_) => rx2,
                         Err(_) => panic!("Failed to send signature request"),
                     }
                 })).unwrap().signature;

        assert!(verify_signature(&my_message[..], &public_key_from_client, &signature));
    }

    // TODO: Add tests that check "concurrency": Multiple clients that send requests.
}
