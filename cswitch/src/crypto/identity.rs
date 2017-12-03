extern crate ring;
extern crate untrusted;

use std::fmt;

use self::ring::signature;

const PUBLIC_KEY_LEN: usize = 32;
const SIGNATURE_LEN:  usize = 64;

#[derive(Debug, PartialEq, Eq, Hash, Clone)]
pub struct PublicKey([u8; PUBLIC_KEY_LEN]);

impl AsRef<[u8]> for PublicKey {
    #[inline]
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
    }
}

// We had to implement Debug and PartialEq ourselves here,
// because PartialEq and Debug traits are not automatically implemented
// for size larger than 32.
pub struct Signature([u8; SIGNATURE_LEN]);

impl AsRef<[u8]> for Signature {
    #[inline]
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
    }
}

// ========== Debug / PartialEq ==========

impl fmt::Debug for Signature {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Debug::fmt(&&self.0[..], f)
    }
}

impl PartialEq for Signature {
    #[inline]
    fn eq(&self, other: &Signature) -> bool {
        for i in 0 .. SIGNATURE_LEN {
            if self.0[i] != other.0[i] {
                return false;
            }
        }
        true
    }
}

/// A generic interface for signing and verifying messages.
pub trait Identity {
    /// Verify a signature of a given message
    // fn verify_signature(&self, message: &[u8], 
    //                     public_key: &PublicKey, signature: &Signature) -> bool;
    /// Create a signature for a given message using private key.
    fn sign_message(&self, message: &[u8]) -> Signature;
    /// Get our public identity
    fn get_public_key(&self) -> PublicKey;
}


pub struct SoftwareEd25519Identity {
    key_pair: signature::Ed25519KeyPair,
}

impl SoftwareEd25519Identity {
    pub fn from_pkcs8(pkcs8_bytes: &[u8]) -> Result<Self,()> {
        let key_pair = match signature::Ed25519KeyPair::from_pkcs8(
            untrusted::Input::from(&pkcs8_bytes)) {
            Ok(key_pair) => key_pair,
            Err(ring::error::Unspecified) => return Err(())
        };

        Ok(SoftwareEd25519Identity {
            key_pair,
        })
    }
}

pub fn verify_signature(message: &[u8],
                        public_key: &PublicKey, signature: &Signature) -> bool {

    let public_key = untrusted::Input::from(&public_key.0);
    let message    = untrusted::Input::from(message);
    let signature  = untrusted::Input::from(&signature.0);
    match signature::verify(&signature::ED25519, public_key, message, signature) {
        Ok(()) => true,
        Err(ring::error::Unspecified) => false,
    }
}


impl Identity for SoftwareEd25519Identity {

    fn sign_message(&self, message: &[u8]) -> Signature {
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



#[cfg(test)]
mod tests {
    use super::*;
    use self::ring::test::rand::FixedByteRandom;

    #[test]
    fn test_get_public_key_sanity() {

        // let secure_rand = DummyRandom::new(&[1,2,3,4,5]);
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

        let signature = id.sign_message(message);
        let public_key = id.get_public_key();
        // println!("public_key = {:?}", public_key);
        // println!("signature = {:?}", signature);

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
        let signature1 = id1.sign_message(message);

        let public_key1 = id1.get_public_key();
        assert!(verify_signature(message, &public_key1, &signature1));

    }
}

