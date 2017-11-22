extern crate ring;

use std::iter;
use self::ring::aead::{seal_in_place, open_in_place, SealingKey, OpeningKey, CHACHA20_POLY1305};
use ::identity::{SymmetricKey, SYMMETRIC_KEY_LEN};

// Length of nonce for CHACHA20_POLY1305
const NONCE_LEN: usize = 12;
// LENGTH OF tag for CHACHA20_POLY1305
const TAG_LEN: usize = 16;

struct EncNonce(pub [u8; NONCE_LEN]);


#[derive(Debug)]
enum SymmetricEncError {
    EncryptionError,
    DecryptionError,
}


struct Encryptor {
    sealing_key: SealingKey,
}

impl Encryptor {
    /// Create a new encryptor object. This object can encrypt messages.
    pub fn new(symmetric_key: &SymmetricKey) -> Self {
        Encryptor {
            sealing_key: SealingKey::new(&CHACHA20_POLY1305, &symmetric_key.0).unwrap(),
        }
    }

    /// Encrypt a message. The nonce must be unique.
    pub fn encrypt(&self, plain_msg: &[u8], nonce: &EncNonce) -> Result<Vec<u8>, SymmetricEncError> {
        // Put the nonce in the beginning of the resulting buffer:
        let mut msg_buffer = nonce.0.to_vec();
        msg_buffer.extend(plain_msg);
        // Extend the message with TAG_LEN zeroes. This leaves space for the tag:
        msg_buffer.extend(iter::repeat(0).take(TAG_LEN).collect::<Vec<u8>>());
        let ad: [u8; 0] = [];
        match seal_in_place(&self.sealing_key, &nonce.0, &ad, &mut msg_buffer[NONCE_LEN .. ], TAG_LEN) {
            Err(ring::error::Unspecified) => Err(SymmetricEncError::EncryptionError),
            Ok(length) => Ok(msg_buffer[.. NONCE_LEN + length] .to_vec())
        }
    }
}

struct Decryptor {
    opening_key: OpeningKey,
}

impl Decryptor {
    /// Create a new decryptor object. This object can decrypt messages.
    pub fn new(symmetric_key: &SymmetricKey) -> Self {
        Decryptor {
            opening_key: OpeningKey::new(&CHACHA20_POLY1305, &symmetric_key.0).unwrap(),
        }
    }

    pub fn decrypt(&self, cipher_msg: &[u8]) -> Result<Vec<u8>, SymmetricEncError> {
        let nonce = &cipher_msg[.. NONCE_LEN];
        let mut msg_buffer = cipher_msg[NONCE_LEN .. ].to_vec();
        let ad: [u8; 0] = [];

        match open_in_place(&self.opening_key, nonce, &ad, 0, &mut msg_buffer) {
            Ok(slice) => Ok(slice.to_vec()),
            Err(ring::error::Unspecified) => Err(SymmetricEncError::DecryptionError),
        }
    }
}


// TODO: Write basic tests.

