use chacha20poly1305::{
    self as chacha,
    aead::{Aead, NewAead},
};
use std::iter;

use crate::error::CryptoError;

use common::big_array::BigArray;

// Length of secret key for CHACHA20_POLY1305
pub const SYMMETRIC_KEY_LEN: usize = 32;
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
    cipher: chacha::ChaCha20Poly1305,
    nonce_counter: EncryptNonceCounter,
}

impl Encryptor {
    /// Create a new encryptor object. This object can encrypt messages.
    pub fn new(symmetric_key: &SymmetricKey) -> Result<Self, CryptoError> {
        let chacha_key = chacha::Key::from_slice(symmetric_key.as_array_ref());
        Ok(Encryptor {
            cipher: chacha::ChaCha20Poly1305::new(chacha_key),
            nonce_counter: EncryptNonceCounter::new(),
        })
    }

    /// Encrypt a message. The nonce must be unique.
    pub fn encrypt(&mut self, plain_msg: &[u8]) -> Result<Vec<u8>, CryptoError> {
        let nonce_vec = self.nonce_counter.next_nonce().0.to_vec();
        let nonce = chacha::Nonce::from_slice(&nonce_vec);

        let raw_ciphertext = self
            .cipher
            .encrypt(nonce, plain_msg)
            .map_err(|_| CryptoError)?;

        let mut ciphertext = nonce_vec;
        ciphertext.extend(raw_ciphertext);
        Ok(ciphertext)
    }
}

/// A structure used for decrypting messages with a given symmetric key.
pub struct Decryptor {
    cipher: chacha::ChaCha20Poly1305,
    nonce_counter: EncryptNonceCounter,
}

impl Decryptor {
    /// Create a new decryptor object. This object can decrypt messages.
    pub fn new(symmetric_key: &SymmetricKey) -> Result<Self, CryptoError> {
        let chacha_key = chacha::Key::from_slice(symmetric_key.as_array_ref());
        Ok(Decryptor {
            cipher: chacha::ChaCha20Poly1305::new(chacha_key),
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
        let nonce = chacha::Nonce::from_slice(&enc_nonce);

        // let mut msg_buffer = cipher_msg[ENC_NONCE_LEN..].to_vec();

        self.cipher
            .decrypt(nonce, &cipher_msg[ENC_NONCE_LEN..])
            .map_err(|_| CryptoError)
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
