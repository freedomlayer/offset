use std::cmp::Ordering;

use proto::crypto::{PrivateKey, PublicKey, Signature};

use crate::error::CryptoError;
use crate::hash::sha_512_256;
use crate::rand::{CryptoRandom, RandGen};

/// Check if one public key is "lower" than another.
/// This is used to decide which side begins the token channel.
pub fn compare_public_key(pk1: &PublicKey, pk2: &PublicKey) -> Ordering {
    sha_512_256(pk1).cmp(&sha_512_256(pk2))
}

impl RandGen for PrivateKey {
    fn rand_gen(crypt_rng: &mut impl CryptoRandom) -> Self {
        PrivateKey::from(ed25519_dalek::SecretKey::generate(crypt_rng).to_bytes())
    }
}

/// A generic interface for signing and verifying messages.
pub trait Identity {
    /// Create a signature for a given message using private key.
    fn sign(&self, message: &[u8]) -> Signature;
    /// Get our public identity
    fn get_public_key(&self) -> PublicKey;
}

pub struct SoftwareEd25519Identity {
    key_pair: ed25519_dalek::Keypair,
}

/// Derive a PublicKey from a PrivateKey
pub fn derive_public_key(private_key: &PrivateKey) -> Result<PublicKey, CryptoError> {
    let dalek_secret_key =
        ed25519_dalek::SecretKey::from_bytes(&private_key).map_err(|_| CryptoError)?;

    let dalek_public_key: ed25519_dalek::PublicKey =
        ed25519_dalek::PublicKey::from(&dalek_secret_key);

    Ok(PublicKey::from(dalek_public_key.to_bytes()))
}

impl SoftwareEd25519Identity {
    pub fn from_private_key(private_key: &PrivateKey) -> Result<Self, CryptoError> {
        let secret = ed25519_dalek::SecretKey::from_bytes(&private_key).map_err(|_| CryptoError)?;
        let public = ed25519_dalek::PublicKey::from(&secret);

        let key_pair = ed25519_dalek::Keypair { secret, public };
        Ok(SoftwareEd25519Identity { key_pair })
    }
}

pub fn verify_signature(message: &[u8], public_key: &PublicKey, signature: &Signature) -> bool {
    let dalek_public_key =
        if let Ok(dalek_public_key) = ed25519_dalek::PublicKey::from_bytes(&public_key) {
            dalek_public_key
        } else {
            return false;
        };

    let dalek_signature =
        if let Ok(dalek_signature) = ed25519_dalek::Signature::from_bytes(&signature) {
            dalek_signature
        } else {
            return false;
        };

    dalek_public_key.verify(message, &dalek_signature).is_ok()
}

impl Identity for SoftwareEd25519Identity {
    fn sign(&self, message: &[u8]) -> Signature {
        let dalek_signature = self.key_pair.sign(message);
        Signature::from(dalek_signature.to_bytes())
    }

    fn get_public_key(&self) -> PublicKey {
        PublicKey::from(self.key_pair.public.to_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::DummyRandom;

    use crate::rand::RandGen;

    #[test]
    fn test_get_public_key_sanity() {
        let mut secure_rand = DummyRandom::new(&[1, 2, 3, 4, 5]);
        let private_key = PrivateKey::rand_gen(&mut secure_rand);
        let public_key0 = derive_public_key(&private_key).unwrap();
        let id = SoftwareEd25519Identity::from_private_key(&private_key).unwrap();

        let public_key1 = id.get_public_key();
        let public_key2 = id.get_public_key();

        assert_eq!(public_key0, public_key2);
        assert_eq!(public_key1, public_key2);
    }

    #[test]
    fn test_sign_verify_self() {
        let mut secure_rand = DummyRandom::new(&[1, 2, 3, 4, 6]);
        let private_key = PrivateKey::rand_gen(&mut secure_rand);
        let id = SoftwareEd25519Identity::from_private_key(&private_key).unwrap();

        let message = b"This is a message";

        let signature = id.sign(message);
        let public_key = id.get_public_key();

        assert!(verify_signature(message, &public_key, &signature));
    }

    #[test]
    fn test_sign_verify_other() {
        let mut secure_rand = DummyRandom::new(&[1, 2, 3, 4, 7]);
        // let pkcs8 = signature::Ed25519KeyPair::generate_pkcs8(&secure_rand).unwrap();
        let private_key = PrivateKey::rand_gen(&mut secure_rand);
        let id1 = SoftwareEd25519Identity::from_private_key(&private_key).unwrap();

        let mut secure_rand = DummyRandom::new(&[1, 2, 3, 4, 8]);
        // let pkcs8 = signature::Ed25519KeyPair::generate_pkcs8(&secure_rand).unwrap();
        let private_key = PrivateKey::rand_gen(&mut secure_rand);
        let id2 = SoftwareEd25519Identity::from_private_key(&private_key).unwrap();

        let message = b"This is a message";
        let signature1 = id1.sign(message);
        let public_key2 = id2.get_public_key();

        assert!(!verify_signature(message, &public_key2, &signature1));
    }
}
