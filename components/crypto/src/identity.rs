// use derive_more::*;
use ring::signature;
use std::cmp::Ordering;

use proto::crypto::{PrivateKey, PublicKey, Signature};

use crate::error::CryptoError;
use crate::hash::sha_512_256;

/// Check if one public key is "lower" than another.
/// This is used to decide which side begins the token channel.
pub fn compare_public_key(pk1: &PublicKey, pk2: &PublicKey) -> Ordering {
    sha_512_256(pk1).cmp(&sha_512_256(pk2))
}

/*
// TODO: Could implement RandGen instead:
/// Generate a pkcs8 key pair
pub fn generate_private_key<R: CryptoRandom>(rng: &R) -> PrivateKey {
    PrivateKey::from(&ring::signature::Ed25519KeyPair::generate_pkcs8(rng).unwrap())
}
*/

/// A generic interface for signing and verifying messages.
pub trait Identity {
    /// Verify a signature of a given message
    // fn verify_signature(&self, message: &[u8],
    //                     public_key: &PublicKey, signature: &Signature) -> bool;
    /// Create a signature for a given message using private key.
    fn sign(&self, message: &[u8]) -> Signature;
    /// Get our public identity
    fn get_public_key(&self) -> PublicKey;
}

pub struct SoftwareEd25519Identity {
    key_pair: signature::Ed25519KeyPair,
}

/// Derive a PublicKey from a PrivateKey
pub fn derive_public_key(private_key: &PrivateKey) -> Result<PublicKey, CryptoError> {
    let key_pair = signature::Ed25519KeyPair::from_pkcs8(untrusted::Input::from(&private_key))?;

    let mut public_key = PublicKey::default();
    let public_key_array = &mut public_key;

    let public_key_ref = key_pair.public_key_bytes();
    public_key_array.clone_from_slice(public_key_ref);

    Ok(public_key)
}

impl SoftwareEd25519Identity {
    pub fn from_private_key(private_key: &PrivateKey) -> Result<Self, CryptoError> {
        // TODO: Possibly use as_ref() for private_key later.
        // Currently we have no blanket implementation that includes PrivateKey, because it is too
        // large (85 bytes!)
        let key_pair = signature::Ed25519KeyPair::from_pkcs8(untrusted::Input::from(&private_key))?;

        Ok(SoftwareEd25519Identity { key_pair })
    }
}

pub fn verify_signature(message: &[u8], public_key: &PublicKey, signature: &Signature) -> bool {
    let public_key = untrusted::Input::from(&public_key);
    let message = untrusted::Input::from(message);
    let signature = untrusted::Input::from(&signature);

    signature::verify(&signature::ED25519, public_key, message, signature).is_ok()
}

impl Identity for SoftwareEd25519Identity {
    fn sign(&self, message: &[u8]) -> Signature {
        let mut signature = Signature::default();

        // let mut sig_array = [0; Signature::len()];
        let sig_array = &mut signature;
        let sig = self.key_pair.sign(message);
        let sig_ref = sig.as_ref();
        // assert_eq!(sig_ref.len(), Signature::len());
        sig_array.clone_from_slice(sig_ref);
        signature
    }

    fn get_public_key(&self) -> PublicKey {
        let mut public_key = PublicKey::default();
        let public_key_array = &mut public_key;

        // let mut public_key_array = [0; PublicKey::len()];
        let public_key_ref = self.key_pair.public_key_bytes();
        // assert_eq!(public_key_ref.len(), PublicKey::len());
        public_key_array.clone_from_slice(public_key_ref);
        public_key
        // PublicKey(public_key_array)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ring::test::rand::FixedByteRandom;

    use crate::rand::RandGen;

    #[test]
    fn test_get_public_key_sanity() {
        let secure_rand = FixedByteRandom { byte: 0x1 };
        let private_key = PrivateKey::rand_gen(&secure_rand);
        let public_key0 = derive_public_key(&private_key).unwrap();
        let id = SoftwareEd25519Identity::from_private_key(&private_key).unwrap();

        let public_key1 = id.get_public_key();
        let public_key2 = id.get_public_key();

        assert_eq!(public_key0, public_key2);
        assert_eq!(public_key1, public_key2);
    }

    #[test]
    fn test_sign_verify_self() {
        let secure_rand = FixedByteRandom { byte: 0x1 };
        let private_key = PrivateKey::rand_gen(&secure_rand);
        let id = SoftwareEd25519Identity::from_private_key(&private_key).unwrap();

        let message = b"This is a message";

        let signature = id.sign(message);
        let public_key = id.get_public_key();

        assert!(verify_signature(message, &public_key, &signature));
    }

    #[test]
    fn test_sign_verify_other() {
        let secure_rand = FixedByteRandom { byte: 0x2 };
        // let pkcs8 = signature::Ed25519KeyPair::generate_pkcs8(&secure_rand).unwrap();
        let private_key = PrivateKey::rand_gen(&secure_rand);
        let id1 = SoftwareEd25519Identity::from_private_key(&private_key).unwrap();

        let secure_rand = FixedByteRandom { byte: 0x3 };
        // let pkcs8 = signature::Ed25519KeyPair::generate_pkcs8(&secure_rand).unwrap();
        let private_key = PrivateKey::rand_gen(&secure_rand);
        let id2 = SoftwareEd25519Identity::from_private_key(&private_key).unwrap();

        let message = b"This is a message";
        let signature1 = id1.sign(message);
        let public_key2 = id2.get_public_key();

        assert!(!verify_signature(message, &public_key2, &signature1));
    }
}
