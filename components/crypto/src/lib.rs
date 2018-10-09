#![feature(try_from)]


extern crate ring;
extern crate bytes;
extern crate cswitch_utils as utils;
extern crate serde;
#[macro_use]
extern crate serde_derive;
extern crate base64;
extern crate rand;
extern crate byteorder;

#[macro_use]
pub mod macros;

pub mod dh;
pub mod hash;
pub mod identity;
pub mod crypto_rand;
pub mod sym_encrypt;
pub mod test_utils;
pub mod uid;
pub mod nonce_window;



#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CryptoError;

impl From<::ring::error::Unspecified> for CryptoError {
    fn from(_: ::ring::error::Unspecified) -> CryptoError { CryptoError }
}

impl ::std::fmt::Display for CryptoError {
    fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
        f.write_str("crypto error")
    }
}

impl ::std::error::Error for CryptoError {
    #[inline]
    fn description(&self) -> &str { "crypto error" }

    #[inline]
    fn cause(&self) -> Option<&::std::error::Error> { None }
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
