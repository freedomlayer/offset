#![deny(trivial_numeric_casts, warnings)]
#![allow(intra_doc_link_resolution_failure)]
#![allow(
    clippy::too_many_arguments,
    clippy::implicit_hasher,
    clippy::module_inception
)]
// TODO: disallow clippy::too_many_arguments

#[macro_use]
extern crate common;
#[macro_use]
extern crate serde_derive;

pub mod crypto_rand;
pub mod dh;
pub mod hash;
pub mod identity;
pub mod invoice_id;
pub mod nonce_window;
pub mod sym_encrypt;
pub mod test_utils;
pub mod uid;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CryptoError;

impl From<::ring::error::Unspecified> for CryptoError {
    fn from(_: ::ring::error::Unspecified) -> CryptoError {
        CryptoError
    }
}

impl ::std::fmt::Display for CryptoError {
    fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
        f.write_str("crypto error")
    }
}

impl ::std::error::Error for CryptoError {
    #[inline]
    fn description(&self) -> &str {
        "crypto error"
    }

    #[inline]
    fn cause(&self) -> Option<&::std::error::Error> {
        None
    }
}

/// Increase the bytes represented number by 1.
///
/// Reference: `libsodium/sodium/utils.c#L241`
#[inline]
pub fn increase_nonce(nonce: &mut [u8]) {
    let mut c: u16 = 1;
    for i in nonce {
        c += u16::from(*i);
        *i = c as u8;
        c >>= 8;
    }
}
