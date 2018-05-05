use crypto::CryptoError;

#[derive(Debug)]
pub enum HandshakeError {
    AlreadyInProgress,

    SessionExists,

    NotAllowed,

    NoSuchSession,

    InvalidKey,

    InvalidSignature,

    Incomplete,

    InvalidTransfer,

    Crypto(CryptoError),
}