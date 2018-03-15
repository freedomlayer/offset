extern crate untrusted;

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
        let mut salt = Salt::default();

        if crypt_rng.fill(&mut salt).is_ok() {
            Ok(salt)
        } else {
            Err(CryptoError)
        }
    }
}

pub struct DhPrivateKey(EphemeralPrivateKey);

impl DhPrivateKey {
    /// Create a new ephemeral private key.
    pub fn new<R: SecureRandom>(rng: &R) -> Result<DhPrivateKey, CryptoError> {
        Ok(DhPrivateKey(EphemeralPrivateKey::generate(&agreement::X25519, rng)?))
    }

    /// Compute public key from our private key.
    /// The public key will be sent to remote side.
    pub fn compute_public_key(&self) -> Result<DhPublicKey, CryptoError> {
        let mut public_key = DhPublicKey([0_u8; DH_PUBLIC_KEY_LEN]);

        if self.0.compute_public_key(&mut public_key).is_ok() {
            Ok(public_key)
        } else {
            Err(CryptoError)
        }
    }

    /// Derive a symmetric key from our private key and remote's public key.
    pub fn derive_symmetric_key(
        self,
        remote_public_key: DhPublicKey,
        sent_salt: Salt,
        recv_salt: Salt,
    ) -> Result<(SymmetricKey, SymmetricKey), CryptoError> {
        let u_remote_public_key = untrusted::Input::from(&remote_public_key);

        let kdf = |shared_key: &[u8]| -> Result<(SymmetricKey, SymmetricKey), CryptoError> {
            if shared_key.len() != SHARED_SECRET_LEN {
                Err(CryptoError)
            } else {
                let sent_sk = SigningKey::new(&digest::SHA512_256, &sent_salt);
                let recv_sk = SigningKey::new(&digest::SHA512_256, &recv_salt);

                let mut send_key = [0x00u8; SYMMETRIC_KEY_LEN];
                let mut recv_key = [0x00u8; SYMMETRIC_KEY_LEN];
                extract_and_expand(&sent_sk, shared_key, &[], &mut send_key);
                extract_and_expand(&recv_sk, shared_key, &[], &mut recv_key);

                Ok((SymmetricKey::from(&send_key), SymmetricKey::from(&recv_key)))
            }
        };

        agreement::agree_ephemeral(
            self.0,
            &agreement::X25519,
            u_remote_public_key,
            CryptoError, kdf,
        )
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
        let dh_private_a = DhPrivateKey::new(&rng).unwrap();
        let dh_private_b = DhPrivateKey::new(&rng).unwrap();

        let public_key_a = dh_private_a.compute_public_key().unwrap();
        let public_key_b = dh_private_b.compute_public_key().unwrap();

        let salt_a = Salt::new(&rng).unwrap();
        let salt_b = Salt::new(&rng).unwrap();

        // Each side derives the symmetric key from the remote's public key
        // and the salt:
        let (send_key_a, recv_key_a) = dh_private_a.derive_symmetric_key(
            public_key_b,
            salt_a.clone(),
            salt_b.clone(),
        ).unwrap();

        let (send_key_b, recv_key_b) = dh_private_b.derive_symmetric_key(
            public_key_a,
            salt_b,
            salt_a,
        ).unwrap();

        // Both sides should get the same derived symmetric key:
        assert_eq!(send_key_a, recv_key_b);
        assert_eq!(send_key_b, recv_key_a)
    }
}
