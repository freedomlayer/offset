use derive_more::*;

#[derive(Clone, Copy, Debug, PartialEq, Display)]
#[display(fmt = "crypto error")]
pub struct CryptoError;

impl From<::ring::error::Unspecified> for CryptoError {
    fn from(_: ::ring::error::Unspecified) -> CryptoError {
        CryptoError
    }
}
