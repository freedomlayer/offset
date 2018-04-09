use crypto::CryptoError;

#[derive(Debug)]
pub enum HandshakeError {
    SessionExist,

    NotAllowed,

    NoSuchSession,

    InvalidKey,

    InvalidSignature,

    Incomplete,

    InvalidTransfer,

    Crypto(CryptoError),
}