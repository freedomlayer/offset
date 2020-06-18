use derive_more::*;

#[derive(Clone, Copy, Debug, PartialEq, Display)]
#[display(fmt = "crypto error")]
pub struct CryptoError;
