extern crate ring;
extern crate untrusted;

use std::mem;

use self::ring::agreement;
use self::ring::agreement::EphemeralPrivateKey;
use self::ring::rand::SecureRandom;
use self::ring::error::Unspecified;
use self::ring::hkdf::extract_and_expand;
use self::ring::hmac::SigningKey;
use super::symmetric_enc::{SymmetricKey, SYMMETRIC_KEY_LEN};

const SALT_LEN: usize = 32;
const DH_PUBLIC_KEY_LEN: usize = 32;
const SHARED_SECRET_LEN: usize = 32;

#[derive(Debug, PartialEq, Eq, Hash, Clone)]
pub struct Salt([u8; SALT_LEN]);

impl Salt {
    pub fn new<R: SecureRandom>(crypt_rng: &R) -> Self {
        let mut inner_salt = [0_u8; SALT_LEN];
        crypt_rng.fill(&mut inner_salt);
        Salt(inner_salt)
    }

    // TODO: Migrate to try_from as soon as it stable
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, ()> {
        if bytes.len() != SALT_LEN {
            Err(())
        } else {
            let mut salt_bytes = [0; SALT_LEN];
            salt_bytes.clone_from_slice(bytes);
            Ok(Salt(salt_bytes))
        }
    }

    #[inline]
    pub fn as_bytes(&self) -> &[u8] {
        self.as_ref()
    }
}

impl AsRef<[u8]> for Salt {
    #[inline]
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
    }
}

#[derive(Debug, PartialEq, Eq, Hash, Clone)]
pub struct DhPublicKey([u8; DH_PUBLIC_KEY_LEN]);

impl DhPublicKey {
    // TODO: Migrate to try_from as soon as it stable
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, ()> {
        if bytes.len() != DH_PUBLIC_KEY_LEN {
            Err(())
        } else {
            let mut dh_public_key_bytes = [0; DH_PUBLIC_KEY_LEN];
            dh_public_key_bytes.clone_from_slice(bytes);
            Ok(DhPublicKey(dh_public_key_bytes))
        }
    }

    #[inline]
    pub fn as_bytes(&self) -> &[u8] {
        self.as_ref()
    }
}

impl AsRef<[u8]> for DhPublicKey {
    #[inline]
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
    }
}

pub struct DhPrivateKey {
    dh_private_key: EphemeralPrivateKey,
}

impl DhPrivateKey {
    /// Create a new ephemeral private key
    pub fn new<R: SecureRandom>(crypt_rng: &R) -> Self {
        let dh_pk = match agreement::EphemeralPrivateKey::generate(
                &agreement::X25519, crypt_rng) {
            Ok(dh_pk) => dh_pk,
            Err(Unspecified) => unreachable!(),
        };

        DhPrivateKey {
            dh_private_key: dh_pk,
        }
    }

    /// Compute public key from our private key.
    /// The public key will be sent to remote side.
    pub fn compute_public_key(&self) -> DhPublicKey {
        let mut public_key = DhPublicKey([0_u8; DH_PUBLIC_KEY_LEN]);
        match self.dh_private_key.compute_public_key(&mut public_key.0) {
            Ok(()) => public_key,
            Err(Unspecified) => unreachable!(),
        }
    }

    /// Derive a symmetric key from our private key and remote's public key.
    pub fn derive_symmetric_key(&self, public_key: &DhPublicKey, salt: &Salt) -> SymmetricKey {
        let u_public_key = untrusted::Input::from(&public_key.0);

        // Force a copy of our private key, so that we can use it more than once.
        // This is a hack due to current limitation of the *ring* crypto library.
        let dh_private_key: EphemeralPrivateKey =
            unsafe { mem::transmute_copy(&self.dh_private_key) };

        // Perform diffie hellman:
        let key_material_res = agreement::agree_ephemeral(dh_private_key,
                    &agreement::X25519, u_public_key,
                    ring::error::Unspecified ,|key_material| {
                        assert_eq!(key_material.len(), SHARED_SECRET_LEN);
                        let mut shared_secret_array = [0; SHARED_SECRET_LEN];
                        shared_secret_array.clone_from_slice(key_material);
                        Ok(shared_secret_array)
                    });

        let shared_secret_array = match key_material_res {
            Ok(key_material) => key_material,
            _ => unreachable!(),
        };

        // Add the salt to the raw_secret using hkdf:
        let skey_salt = SigningKey::new(&ring::digest::SHA512_256, &salt.0);
        let info: [u8; 0] = [];
        let mut out = [0_u8; SYMMETRIC_KEY_LEN];
        extract_and_expand(&skey_salt, &shared_secret_array, &info, &mut out);
        SymmetricKey(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::test_utils::DummyRandom;

    #[test]
    fn test_new_salt() {
        let rng = DummyRandom::new(&[1,2,3,4,6]);
        let salt1 = Salt::new(&rng);
        let salt2 = Salt::new(&rng);

        assert_ne!(salt1, salt2);
    }

    #[test]
    fn test_derive_symmetric_key() {
        let rng = DummyRandom::new(&[1,2,3,4,5]);
        let dh_private_key1 = DhPrivateKey::new(&rng);
        let dh_private_key2 = DhPrivateKey::new(&rng);

        let public_key1 = dh_private_key1.compute_public_key();
        let public_key2 = dh_private_key2.compute_public_key();

        // Same salt for both sides:
        let salt = Salt::new(&rng);

        // Each side derives the symmetric key from the remote's public key
        // and the salt:
        let symmetric_key1 = dh_private_key1.derive_symmetric_key(
            &public_key2, &salt);
        let symmetric_key2 = dh_private_key2.derive_symmetric_key(
            &public_key1, &salt);

        // Both sides should get the same derived symmetric key:
        assert_eq!(symmetric_key1, symmetric_key2);
    }
}

