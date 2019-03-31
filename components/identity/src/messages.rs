use crypto::identity::{PublicKey, Signature};
use futures::channel::oneshot;

/// The response from security module client to security module.
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
pub struct ResponseSignature {
    pub signature: Signature,
}

/// Return the identity public key.
pub struct ResponsePublicKey {
    pub public_key: PublicKey,
}
