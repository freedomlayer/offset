extern crate ring;

use std::iter;
use self::ring::aead::{seal_in_place, open_in_place, SealingKey, OpeningKey, CHACHA20_POLY1305};
use self::ring::rand::SecureRandom;

pub const SYMMETRIC_KEY_LEN: usize = 32;

#[derive(Debug, PartialEq)]
pub struct SymmetricKey(pub [u8; SYMMETRIC_KEY_LEN]);


// Length of nonce for CHACHA20_POLY1305
const ENC_NONCE_LEN: usize = 12;
// LENGTH OF tag for CHACHA20_POLY1305
const TAG_LEN: usize = 16;

#[derive(Clone)]
pub struct EncNonce(pub [u8; ENC_NONCE_LEN]);

/// increase an array represented number by 1.
fn inc_array_num(array_num: &mut [u8]) {
    for byte in array_num {
        if *byte == 0xff {
            *byte = 0;
        } else {
            *byte += 1;
            // No carry, we break:
            break;
        }
    }
}

pub struct EncNonceCounter {
    enc_nonce: EncNonce,
}

impl EncNonceCounter {
    pub fn new<R: SecureRandom>(crypt_rng: &mut R) -> Self {
        let mut enc_nonce = EncNonce([0_u8; ENC_NONCE_LEN]);
        // Generate a random initial EncNonce:
        crypt_rng.fill(&mut enc_nonce.0).unwrap();
        EncNonceCounter {
            enc_nonce,
        }
    }

    /// Get a new nonce.
    pub fn get_nonce(&mut self) -> EncNonce {
        let export_nonce = self.enc_nonce.clone();
        inc_array_num(&mut self.enc_nonce.0);
        export_nonce
    }
}

#[derive(Debug)]
pub enum SymmetricEncError {
    EncryptionError,
    DecryptionError,
}


/// A structure used for encrypting messages with a given symmetric key.
/// Maintains internal state of an increasing nonce counter.
pub struct Encryptor {
    sealing_key: SealingKey,
    enc_nonce_counter: EncNonceCounter,
}

impl Encryptor {
    /// Create a new encryptor object. This object can encrypt messages.
    pub fn new(symmetric_key: &SymmetricKey, enc_nonce_counter: EncNonceCounter) -> Self {
        Encryptor {
            sealing_key: SealingKey::new(&CHACHA20_POLY1305, &symmetric_key.0).unwrap(),
            enc_nonce_counter,
        }
    }

    /// Encrypt a message. The nonce must be unique.
    pub fn encrypt(&mut self, plain_msg: &[u8]) -> Result<Vec<u8>, SymmetricEncError> {
        // Put the nonce in the beginning of the resulting buffer:
        let enc_nonce = self.enc_nonce_counter.get_nonce();
        let mut msg_buffer = enc_nonce.0.to_vec();
        msg_buffer.extend(plain_msg);
        // Extend the message with TAG_LEN zeroes. This leaves space for the tag:
        msg_buffer.extend(iter::repeat(0).take(TAG_LEN).collect::<Vec<u8>>());
        let ad: [u8; 0] = [];
        match seal_in_place(&self.sealing_key, &enc_nonce.0, &ad, &mut msg_buffer[ENC_NONCE_LEN .. ], TAG_LEN) {
            Err(ring::error::Unspecified) => Err(SymmetricEncError::EncryptionError),
            Ok(length) => Ok(msg_buffer[.. ENC_NONCE_LEN + length] .to_vec())
        }
    }
}

/// A structure used for decrypting messages with a given symmetric key.
pub struct Decryptor {
    opening_key: OpeningKey,
}

impl Decryptor {
    /// Create a new decryptor object. This object can decrypt messages.
    pub fn new(symmetric_key: &SymmetricKey) -> Self {
        Decryptor {
            opening_key: OpeningKey::new(&CHACHA20_POLY1305, &symmetric_key.0).unwrap(),
        }
    }

    /// Decrypt and authenticate a message.
    pub fn decrypt(&self, cipher_msg: &[u8]) -> Result<Vec<u8>, SymmetricEncError> {
        let enc_nonce = &cipher_msg[.. ENC_NONCE_LEN];
        let mut msg_buffer = cipher_msg[ENC_NONCE_LEN .. ].to_vec();
        let ad: [u8; 0] = [];

        match open_in_place(&self.opening_key, enc_nonce, &ad, 0, &mut msg_buffer) {
            Ok(slice) => Ok(slice.to_vec()),
            Err(ring::error::Unspecified) => Err(SymmetricEncError::DecryptionError),
        }
    }
}

#[cfg(test)]
mod tests {
    extern crate rand;
    use super::*;
    // use self::rand::{StdRng};
    use self::ring::test::rand::FixedByteRandom;
    
    #[test]
    fn test_inc_array_num_basic() {
        let mut array_num = [0,0,0,0];
        inc_array_num(&mut array_num);
        assert_eq!(array_num, [1,0,0,0]);

        for _ in 0 .. 0xff {
            inc_array_num(&mut array_num);
        }
        assert_eq!(array_num, [0,1,0,0]);
    }

    #[test]
    fn test_inc_array_num_wraparound() {
        let mut array_num = [0xff,0xff,0xff,0xff];
        inc_array_num(&mut array_num);
        assert_eq!(array_num, [0,0,0,0]);
    }

    #[test]
    fn test_encryptor_decryptor() {
        let symmetric_key = SymmetricKey([1; SYMMETRIC_KEY_LEN]);

        // let rng_seed: &[_] = &[1,2,3,4,5,6];
        // let mut rng: StdRng = rand::SeedableRng::from_seed(rng_seed);
        let mut rng = FixedByteRandom { byte: 0x10 };
        let enc_nonce_counter = EncNonceCounter::new(&mut rng);
        let mut encryptor = Encryptor::new(&symmetric_key, enc_nonce_counter);

        let decryptor = Decryptor::new(&symmetric_key);


        let plain_msg = b"Hello world!";
        let cipher_msg = encryptor.encrypt(plain_msg).unwrap();
        let decrypted_msg = decryptor.decrypt(&cipher_msg).unwrap();

        assert_eq!(plain_msg, &decrypted_msg[..]);
    }
}


