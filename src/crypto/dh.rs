extern crate untrusted;

use std::mem;
use std::convert::TryFrom;

use ring::digest;
use ring::agreement::{self, EphemeralPrivateKey};
use ring::rand::SecureRandom;
use ring::hkdf::extract_and_expand;
use ring::hmac::SigningKey;

use super::CryptoError;
use super::sym_encrypt::{SymmetricKey, SYMMETRIC_KEY_LEN};

pub const SALT_LEN: usize = 32;
pub const DH_PUBLIC_KEY_LEN: usize = 32;
pub const SHARED_SECRET_LEN: usize = 32;

define_fixed_bytes!(Salt, SALT_LEN);
define_fixed_bytes!(DhPublicKey, DH_PUBLIC_KEY_LEN);

impl Salt {
    pub fn new<R: SecureRandom>(crypt_rng: &R) -> Result<Salt, CryptoError> {
        let mut salt = Salt::zero();

        if crypt_rng.fill(&mut salt).is_ok() {
            Ok(salt)
        } else {
            Err(CryptoError)
        }
    }
}

pub struct DhPrivateKey {
    inner: EphemeralPrivateKey,
}

impl DhPrivateKey {
    /// Create a new ephemeral private key.
    pub fn new<R: SecureRandom>(rng: &R) -> Result<DhPrivateKey, CryptoError> {
        Ok(DhPrivateKey {
            inner: EphemeralPrivateKey::generate(&agreement::X25519, rng)?
        })
    }

    /// Compute public key from our private key.
    /// The public key will be sent to remote side.
    pub fn compute_public_key(&self) -> Result<DhPublicKey, CryptoError> {
        let mut public_key = DhPublicKey([0_u8; DH_PUBLIC_KEY_LEN]);

        if self.inner.compute_public_key(&mut public_key).is_ok() {
            Ok(public_key)
        } else {
            Err(CryptoError)
        }
    }

    /// Derive a symmetric key from our private key and remote's public key.
    pub fn derive_symmetric_key(&self, public_key: &DhPublicKey, salt: &Salt)
        -> Result<SymmetricKey, CryptoError> {
        let peer_public_key = untrusted::Input::from(&public_key);

        // Force a copy of our private key, so that we can use it more than once.
        // This is a hack due to current limitation of the *ring* utils.crypto library.
        let my_private_key: EphemeralPrivateKey = unsafe {
            mem::transmute_copy(&self.inner)
        };

        let kdf = |shared_key: &[u8]| -> Result<SymmetricKey, CryptoError> {
            if shared_key.len() != SHARED_SECRET_LEN {
                Err(CryptoError)
            } else {
                let sk = SigningKey::new(&digest::SHA512_256, &salt);
                let info: [u8; 0] = [];
                let mut key = [0x00u8; SYMMETRIC_KEY_LEN];
                extract_and_expand(&sk, &shared_key, &[][..], &mut key);

                SymmetricKey::try_from(&key[..]).map_err(|_| CryptoError)
            }
        };

        // Perform Diffie-Hellman and derive the symmetric key
        agreement::agree_ephemeral(my_private_key, &agreement::X25519,
                                   peer_public_key, CryptoError, kdf)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::test_utils::DummyRandom;

    #[test]
    fn test_new_salt() {
        let rng = DummyRandom::new(&[1, 2, 3, 4, 6]);
        let salt1 = Salt::new(&rng).unwrap();
        let salt2 = Salt::new(&rng).unwrap();

        assert_ne!(salt1, salt2);
    }

    #[test]
    fn test_derive_symmetric_key() {
        let rng = DummyRandom::new(&[1, 2, 3, 4, 5]);
        let dh_private_key1 = DhPrivateKey::new(&rng).unwrap();
        let dh_private_key2 = DhPrivateKey::new(&rng).unwrap();

        let public_key1 = dh_private_key1.compute_public_key().unwrap();
        let public_key2 = dh_private_key2.compute_public_key().unwrap();

        // Same salt for both sides:
        let salt = Salt::new(&rng).unwrap();

        // Each side derives the symmetric key from the remote's public key
        // and the salt:
        let symmetric_key1 =
            dh_private_key1.derive_symmetric_key(&public_key2, &salt).unwrap();

        let symmetric_key2 =
            dh_private_key2.derive_symmetric_key(&public_key1, &salt).unwrap();

        // Both sides should get the same derived symmetric key:
        assert_eq!(symmetric_key1, symmetric_key2);
    }
}
