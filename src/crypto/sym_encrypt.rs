use std::iter;

use ring;
use ring::aead::{open_in_place, seal_in_place, CHACHA20_POLY1305, OpeningKey, SealingKey};
use ring::rand::SecureRandom;

use super::{CryptoError, increase_nonce};

pub const SYMMETRIC_KEY_LEN: usize = 32;
// Length of tag for CHACHA20_POLY1305
const TAG_LEN: usize = 16;
// Length of nonce for CHACHA20_POLY1305
const ENC_NONCE_LEN: usize = 12;

define_fixed_bytes!(SymmetricKey, SYMMETRIC_KEY_LEN);

#[derive(Clone)]
pub struct EncryptNonce(pub [u8; ENC_NONCE_LEN]);

pub struct EncryptNonceCounter {
    inner: EncryptNonce,
}

impl EncryptNonceCounter {
    pub fn new<R: SecureRandom>(crypt_rng: &R) -> Result<Self, CryptoError> {
        let mut enc_nonce = EncryptNonce([0_u8; ENC_NONCE_LEN]);
        // Generate a random initial EncNonce:
        if crypt_rng.fill(&mut enc_nonce.0).is_ok() {
            Ok(EncryptNonceCounter { inner: enc_nonce })
        } else {
            Err(CryptoError)
        }
    }

    /// Get a new nonce.
    pub fn next_nonce(&mut self) -> EncryptNonce {
        let export_nonce = self.inner.clone();
        increase_nonce(&mut self.inner.0);
        export_nonce
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
    pub fn new(symmetric_key: &SymmetricKey, nonce_counter: EncryptNonceCounter)
        -> Result<Self, CryptoError> {
        Ok(Encryptor {
            sealing_key: SealingKey::new(&CHACHA20_POLY1305, symmetric_key)?,
            nonce_counter,
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
}

impl Decryptor {
    /// Create a new decryptor object. This object can decrypt messages.
    pub fn new(symmetric_key: &SymmetricKey) -> Result<Self, CryptoError> {
        Ok(Decryptor {
            opening_key: OpeningKey::new(&CHACHA20_POLY1305, symmetric_key)?,
        })
    }

    /// Decrypt and authenticate a message.
    pub fn decrypt(&self, cipher_msg: &[u8]) -> Result<Vec<u8>, CryptoError> {
        let enc_nonce = &cipher_msg[..ENC_NONCE_LEN];
        let mut msg_buffer = cipher_msg[ENC_NONCE_LEN..].to_vec();
        let ad: [u8; 0] = [];

        match open_in_place(&self.opening_key, enc_nonce, &ad, 0, &mut msg_buffer) {
            Ok(slice) => Ok(slice.to_vec()),
            Err(ring::error::Unspecified) => Err(CryptoError),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ring::test::rand::FixedByteRandom;

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
        let rng = FixedByteRandom { byte: 0x10 };
        let enc_nonce_counter = EncryptNonceCounter::new(&rng).unwrap();
        let mut encryptor = Encryptor::new(&symmetric_key, enc_nonce_counter).unwrap();

        let decryptor = Decryptor::new(&symmetric_key).unwrap();

        let plain_msg = b"Hello world!";
        let cipher_msg = encryptor.encrypt(plain_msg).unwrap();
        let decrypted_msg = decryptor.decrypt(&cipher_msg).unwrap();

        assert_eq!(plain_msg, &decrypted_msg[..]);
    }
}
