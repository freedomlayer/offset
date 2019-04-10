use derive_more::*;
use ring::signature;
use std::cmp::Ordering;

use super::CryptoError;
use crate::crypto_rand::CryptoRandom;
use crate::hash::sha_512_256;
use common::big_array::BigArray;

pub const PUBLIC_KEY_LEN: usize = 32;
pub const SIGNATURE_LEN: usize = 64;

define_fixed_bytes!(PublicKey, PUBLIC_KEY_LEN);

#[derive(Clone, Serialize, Deserialize, From)]
pub struct Signature(#[serde(with = "BigArray")] [u8; SIGNATURE_LEN]);

/// Check if one public key is "lower" than another.
/// This is used to decide which side begins the token channel.
pub fn compare_public_key(pk1: &PublicKey, pk2: &PublicKey) -> Ordering {
    sha_512_256(pk1).cmp(&sha_512_256(pk2))
}

impl Signature {
    pub fn zero() -> Signature {
        Signature([0x00u8; SIGNATURE_LEN])
    }
}

/// Generate a pkcs8 key pair
pub fn generate_pkcs8_key_pair<R: CryptoRandom>(rng: &R) -> [u8; 85] {
    ring::signature::Ed25519KeyPair::generate_pkcs8(rng).unwrap()
}

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

impl SoftwareEd25519Identity {
    pub fn from_pkcs8(pkcs8_bytes: &[u8]) -> Result<Self, CryptoError> {
        let key_pair = signature::Ed25519KeyPair::from_pkcs8(untrusted::Input::from(pkcs8_bytes))?;

        Ok(SoftwareEd25519Identity { key_pair })
    }
}

pub fn verify_signature(message: &[u8], public_key: &PublicKey, signature: &Signature) -> bool {
    let public_key = untrusted::Input::from(&public_key.0);
    let message = untrusted::Input::from(message);
    let signature = untrusted::Input::from(&signature.0);

    signature::verify(&signature::ED25519, public_key, message, signature).is_ok()
}

impl Identity for SoftwareEd25519Identity {
    fn sign(&self, message: &[u8]) -> Signature {
        let mut sig_array = [0; SIGNATURE_LEN];
        let sig = self.key_pair.sign(message);
        let sig_ref = sig.as_ref();
        assert_eq!(sig_ref.len(), SIGNATURE_LEN);
        sig_array.clone_from_slice(sig_ref);
        Signature(sig_array)
    }

    fn get_public_key(&self) -> PublicKey {
        let mut public_key_array = [0; PUBLIC_KEY_LEN];
        let public_key_ref = self.key_pair.public_key_bytes();
        assert_eq!(public_key_ref.len(), PUBLIC_KEY_LEN);
        public_key_array.clone_from_slice(public_key_ref);
        PublicKey(public_key_array)
    }
}

// ==================== Convenience for Signature ====================
// Cuz SIGNATURE_LEN > 32, these trait can't be derived automatically.
// ===================================================================

impl AsRef<[u8]> for Signature {
    #[inline]
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl ::std::ops::Deref for Signature {
    type Target = [u8];
    #[inline]
    fn deref(&self) -> &[u8] {
        &self.0
    }
}

impl ::std::ops::DerefMut for Signature {
    #[inline]
    fn deref_mut(&mut self) -> &mut [u8] {
        &mut self.0
    }
}

impl<'a> ::std::convert::From<&'a [u8; SIGNATURE_LEN]> for Signature {
    #[inline]
    fn from(src: &'a [u8; SIGNATURE_LEN]) -> Signature {
        let mut inner = [0x00u8; SIGNATURE_LEN];
        inner.copy_from_slice(&src[..SIGNATURE_LEN]);
        Signature(inner)
    }
}

impl<'a> ::std::convert::TryFrom<&'a [u8]> for Signature {
    type Error = ();

    #[inline]
    fn try_from(src: &'a [u8]) -> Result<Signature, ()> {
        if src.len() < SIGNATURE_LEN {
            Err(())
        } else {
            let mut inner = [0x00u8; SIGNATURE_LEN];
            inner.copy_from_slice(&src[..SIGNATURE_LEN]);
            Ok(Signature(inner))
        }
    }
}

impl<'a> ::std::convert::TryFrom<&'a ::bytes::Bytes> for Signature {
    type Error = ();

    #[inline]
    fn try_from(src: &'a ::bytes::Bytes) -> Result<Signature, ()> {
        if src.len() < SIGNATURE_LEN {
            Err(())
        } else {
            let mut inner = [0x00u8; SIGNATURE_LEN];
            inner.copy_from_slice(&src[..SIGNATURE_LEN]);
            Ok(Signature(inner))
        }
    }
}

impl PartialEq for Signature {
    #[inline]
    fn eq(&self, other: &Signature) -> bool {
        for i in 0..SIGNATURE_LEN {
            if self.0[i] != other.0[i] {
                return false;
            }
        }
        true
    }
}

impl Eq for Signature {}

impl ::std::fmt::Debug for Signature {
    fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
        ::std::fmt::Debug::fmt(&self.0[..], f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ring::test::rand::FixedByteRandom;

    #[test]
    fn test_get_public_key_sanity() {
        let secure_rand = FixedByteRandom { byte: 0x1 };
        let pkcs8 = signature::Ed25519KeyPair::generate_pkcs8(&secure_rand).unwrap();
        let id = SoftwareEd25519Identity::from_pkcs8(&pkcs8).unwrap();

        let public_key1 = id.get_public_key();
        let public_key2 = id.get_public_key();

        assert_eq!(public_key1, public_key2);
    }

    #[test]
    fn test_sign_verify_self() {
        let secure_rand = FixedByteRandom { byte: 0x1 };
        let pkcs8 = signature::Ed25519KeyPair::generate_pkcs8(&secure_rand).unwrap();
        let id = SoftwareEd25519Identity::from_pkcs8(&pkcs8).unwrap();

        let message = b"This is a message";

        let signature = id.sign(message);
        let public_key = id.get_public_key();

        assert!(verify_signature(message, &public_key, &signature));
    }

    #[test]
    fn test_sign_verify_other() {
        let secure_rand = FixedByteRandom { byte: 0x2 };
        let pkcs8 = signature::Ed25519KeyPair::generate_pkcs8(&secure_rand).unwrap();
        let id1 = SoftwareEd25519Identity::from_pkcs8(&pkcs8).unwrap();

        let secure_rand = FixedByteRandom { byte: 0x3 };
        let pkcs8 = signature::Ed25519KeyPair::generate_pkcs8(&secure_rand).unwrap();
        let id2 = SoftwareEd25519Identity::from_pkcs8(&pkcs8).unwrap();

        let message = b"This is a message";
        let signature1 = id1.sign(message);
        let public_key2 = id2.get_public_key();

        assert!(!verify_signature(message, &public_key2, &signature1));
    }
}
