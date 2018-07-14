use crypto::CryptoError;

#[derive(Debug)]
pub enum HandshakeError {
    CryptoError(CryptoError),

    /// The handshake is in progress.
    HandshakeInProgress,

    /// `request_nonce` consisted of a `ResponseNonce` message
    /// can not be found in local request nonce sessions table.
    ///
    /// There are some reasons may cause this error:
    ///
    /// - Localhost sent the nonce before, while it had expired; or
    /// - Localhost never sent this value, a guy may be hacking us.
    RequestNonceSessionNotFound,

    /// Localhost is not allowed to act as initiator in the path.
    LocalhostNotInitiator,

    /// Localhost is not allowed to act as responder in the path.
    LocalhostNotResponder,

    /// The neighbor public key in `ExchangeActive` message can
    /// not be found in local neighbors table.
    UnknownNeighbor,

    /// `responder_rand_nonce` consisted of a `ExchangeActive`
    /// message can not be found in local constant nonce list.
    ///
    /// There are some reasons may cause this error:
    ///
    /// - Localhost sent the nonce before, while it had expired; or
    /// - Localhost never send this value, a guy may be hacking us.
    InvalidResponderNonce,

    /// `prev_hash` consisted of a handshake message can not be found
    /// in local handshake sessions table.
    ///
    /// There are some reason may cause this error:
    ///
    ///  - Localhost sent a message with that hash value, while it had expired; or
    ///  - Localhost never send such a message, a guy may be hacking our system.
    HandshakeSessionNotFound,

    /// Failed to derive symmetric key.
    DeriveSymmetricKeyFailed,

    /// Failed to convert symmetric key to `SealingKey`.
    ConvertToSealingKeyFailed,

    /// Failed to convert symmetric key to `OpeningKey`.
    ConvertToOpeningKeyFailed,

    /// The signature of the message doesn't verify the remote public key.
    SignatureVerificationFailed,

    /// A session already exists, which means we reuse
    /// random value in a time window; or message hash
    /// OR session identifier not unique at the moment.
    SessionAlreadyExists,

    /// Receive a message which report a inconsistent state between local.
    InconsistentState,
}
