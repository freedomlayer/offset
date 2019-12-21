use futures::channel::oneshot;

use proto::crypto::{PublicKey, Signature};

/// The response from security module client to security module.
#[derive(Debug)]
pub enum ToIdentity {
    /// Request to sign a message.
    RequestSignature {
        message: Vec<u8>,
        response_sender: oneshot::Sender<ResponseSignature>,
    },
    /// Request the identity public key.
    RequestPublicKey {
        response_sender: oneshot::Sender<ResponsePublicKey>,
    },
}

/// Return requested signature over a message
#[derive(Debug)]
pub struct ResponseSignature {
    pub signature: Signature,
}

/// Return the identity public key.
#[derive(Debug)]
pub struct ResponsePublicKey {
    pub public_key: PublicKey,
}
