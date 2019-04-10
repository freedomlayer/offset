#![deny(trivial_numeric_casts, warnings)]
#![allow(intra_doc_link_resolution_failure)]
#![allow(
    clippy::too_many_arguments,
    clippy::implicit_hasher,
    clippy::module_inception,
    clippy::new_without_default
)]

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

use derive_more::*;

#[derive(Clone, Copy, Debug, PartialEq, Display)]
#[display(fmt = "crypto error")]
pub struct CryptoError;

impl From<::ring::error::Unspecified> for CryptoError {
    fn from(_: ::ring::error::Unspecified) -> CryptoError {
        CryptoError
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
