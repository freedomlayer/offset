use std::iter;

use quickcheck::{Arbitrary, Gen};

use ring;
use ring::aead::{open_in_place, seal_in_place, OpeningKey, SealingKey, CHACHA20_POLY1305};

use crate::error::CryptoError;

use common::big_array::BigArray;

pub const SYMMETRIC_KEY_LEN: usize = 32;
// Length of tag for CHACHA20_POLY1305
const TAG_LEN: usize = 16;
// Length of nonce for CHACHA20_POLY1305
const ENC_NONCE_LEN: usize = 12;

define_fixed_bytes!(SymmetricKey, SYMMETRIC_KEY_LEN);

#[derive(Clone)]
pub struct EncryptNonce(pub [u8; ENC_NONCE_LEN]);

/// Increase the bytes represented number by 1.
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

pub struct EncryptNonceCounter {
    inner: EncryptNonce,
}

impl EncryptNonceCounter {
    pub fn new() -> Self {
        EncryptNonceCounter {
            inner: EncryptNonce([0_u8; ENC_NONCE_LEN]),
        }
    }

    /// Get a new nonce.
    pub fn next_nonce(&mut self) -> EncryptNonce {
        let export_nonce = self.inner.clone();
        increase_nonce(&mut self.inner.0);
        export_nonce
    }
}

impl AsRef<[u8; ENC_NONCE_LEN]> for EncryptNonceCounter {
    fn as_ref(&self) -> &[u8; ENC_NONCE_LEN] {
        &self.inner.0
    }
}

impl Default for EncryptNonceCounter {
    fn default() -> Self {
        Self::new()
    }
}

/// A structure used for encrypting messages with a given symmetric key.
/// Maintains internal state of an increasing nonce counter.
pub struct Encryptor {
    sealing_key: SealingKey,
    nonce_counter: EncryptNonceCounter,
}

impl Encryptor {
    /// Create a new encryptor object. This object can encrypt messages.
    pub fn new(symmetric_key: &SymmetricKey) -> Result<Self, CryptoError> {
        Ok(Encryptor {
            sealing_key: SealingKey::new(&CHACHA20_POLY1305, symmetric_key)?,
            nonce_counter: EncryptNonceCounter::new(),
        })
    }

    /// Encrypt a message. The nonce must be unique.
    pub fn encrypt(&mut self, plain_msg: &[u8]) -> Result<Vec<u8>, CryptoError> {
        // Put the nonce in the beginning of the resulting buffer:
        let enc_nonce = self.nonce_counter.next_nonce();
        let mut msg_buffer = enc_nonce.0.to_vec();
        msg_buffer.extend(plain_msg);
        // Extend the message with TAG_LEN zeroes. This leaves space for the tag:
        msg_buffer.extend(iter::repeat(0).take(TAG_LEN).collect::<Vec<u8>>());
        let ad: [u8; 0] = [];

        match seal_in_place(
            &self.sealing_key,
            &enc_nonce.0,
            &ad,
            &mut msg_buffer[ENC_NONCE_LEN..],
            TAG_LEN,
        ) {
            Err(ring::error::Unspecified) => Err(CryptoError),
            Ok(length) => Ok(msg_buffer[..ENC_NONCE_LEN + length].to_vec()),
        }
    }
}

/// A structure used for decrypting messages with a given symmetric key.
pub struct Decryptor {
    opening_key: OpeningKey,
    nonce_counter: EncryptNonceCounter,
}

impl Decryptor {
    /// Create a new decryptor object. This object can decrypt messages.
    pub fn new(symmetric_key: &SymmetricKey) -> Result<Self, CryptoError> {
        Ok(Decryptor {
            opening_key: OpeningKey::new(&CHACHA20_POLY1305, symmetric_key)?,
            nonce_counter: EncryptNonceCounter::new(),
        })
    }

    /// Decrypt and authenticate a message.
    pub fn decrypt(&mut self, cipher_msg: &[u8]) -> Result<Vec<u8>, CryptoError> {
        let enc_nonce = &cipher_msg[..ENC_NONCE_LEN];
        if enc_nonce != self.nonce_counter.as_ref() {
            // Nonce doesn't match!
            return Err(CryptoError);
        }

        let mut msg_buffer = cipher_msg[ENC_NONCE_LEN..].to_vec();
        let ad: [u8; 0] = [];

        match open_in_place(&self.opening_key, enc_nonce, &ad, 0, &mut msg_buffer) {
            Ok(slice) => {
                let _ = self.nonce_counter.next_nonce();
                Ok(slice.to_vec())
            }
            Err(ring::error::Unspecified) => Err(CryptoError),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn increase_nonce_basic() {
        let mut nonce = [0, 0, 0, 0];
        increase_nonce(&mut nonce);
        assert_eq!(nonce, [1, 0, 0, 0]);

        for _ in 0..0xff {
            increase_nonce(&mut nonce);
        }
        assert_eq!(nonce, [0, 1, 0, 0]);
    }

    #[test]
    fn increase_nonce_wraparound() {
        let mut array_num = [0xff, 0xff, 0xff, 0xff];
        increase_nonce(&mut array_num);
        assert_eq!(array_num, [0, 0, 0, 0]);
    }

    #[test]
    fn test_encryptor_decryptor() {
        let symmetric_key = SymmetricKey::from(&[1; SYMMETRIC_KEY_LEN]);

        // let rng_seed: &[_] = &[1,2,3,4,5,6];
        // let mut rng: StdRng = rand::SeedableRng::from_seed(rng_seed);
        let mut encryptor = Encryptor::new(&symmetric_key).unwrap();
        let mut decryptor = Decryptor::new(&symmetric_key).unwrap();

        let plain_msg = b"Hello world!";
        let cipher_msg = encryptor.encrypt(plain_msg).unwrap();
        let decrypted_msg = decryptor.decrypt(&cipher_msg).unwrap();

        assert_eq!(plain_msg, &decrypted_msg[..]);
    }
}
