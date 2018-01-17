use crypto::identity::{PublicKey, Signature};

/// The response from security module client to security module.
pub enum ToSecurityModule {
    /// Request to sign a message.
    RequestSignature { message: Vec<u8> },
    /// Request the identity public key.
    RequestPublicKey,
}

/// The response from security module to security module client.
pub enum FromSecurityModule {
    /// The response for `ToSecurityModule::RequestSignature`.
    ResponseSignature { signature: Signature },
    /// The response for `ToSecurityModule::RequestPublicKey`.
    ResponsePublicKey { public_key: PublicKey },
}
