extern crate crypto;

use std::fmt;

use self::crypto::ed25519::{signature, verify, keypair, exchange};

const PUBLIC_KEY_LEN: usize = 32;
const SIGNATURE_LEN: usize = 64;
const SHARED_SECRET_LEN: usize = 32;

#[derive(Debug, PartialEq, Eq, Hash, Clone)]
pub struct PublicKey([u8; PUBLIC_KEY_LEN]);

// We had to implement Debug and PartialEq ourselves here,
// because PartialEq and Debug traits are not automatically implemented
// for size larger than 32.
pub struct Signature([u8; SIGNATURE_LEN]);


impl fmt::Debug for Signature {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Debug::fmt(&&self.0[..], f)
    }
}

impl PartialEq for Signature {
    fn eq(&self, other: &Signature) -> bool {
        for i in 0 .. SIGNATURE_LEN {
            if self.0[i] != other.0[i] {
                return false;
            }
        }
        true
    }
}

#[derive(Debug, PartialEq)]
pub struct SharedSecret([u8; SHARED_SECRET_LEN]);


/// A generic interface for signing and verifying messages.
pub trait Identity {
    /// Verify a signature of a given message
    fn verify_signature(&self, message: &[u8], 
                        public_key: &PublicKey, signature: &Signature) -> bool;
    /// Create a signature for a given message using private key.
    fn sign_message(&self, message: &[u8]) -> Signature;
    /// Get our public identity
    fn get_public_key(&self) -> PublicKey;
    /// Create a shared secret from our identity and the 
    /// public key of a remote user.
    fn gen_shared_secret(&self, public_key: &PublicKey) -> SharedSecret;
}


/// A software powered module for signing and verifying messages.
pub struct SoftwareEd25519Identity {
    private_key: [u8; 64],
    public_key: [u8; 32],
}

impl SoftwareEd25519Identity {
    /// Create a new software powered module.
    /// Generate an identity using the given seed.
    pub fn new(seed: &[u8]) -> SoftwareEd25519Identity {
        // Strange things happen if the seed length is not 32:
        assert_eq!(seed.len(), 32);
        let (private_key, public_key) = keypair(seed);

        SoftwareEd25519Identity {
            private_key,
            public_key,
        }
    }
}


impl Identity for SoftwareEd25519Identity {
    fn verify_signature(&self, message: &[u8], public_key: &PublicKey, signature: &Signature) -> bool {
        verify(message, &public_key.0, &signature.0)

    }

    fn sign_message(&self, message: &[u8]) -> Signature {
        Signature(signature(message, &self.private_key))

    }

    fn get_public_key(&self) -> PublicKey {
        PublicKey(self.public_key)
    }

    fn gen_shared_secret(&self, public_key: &PublicKey) -> SharedSecret {
        SharedSecret(exchange(&public_key.0, &self.private_key))
    }

}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_public_key_sanity() {
        let seed: &[u8] = &[0x26, 0x27, 0xf6, 0x85, 0x97, 0x15, 0xad, 0x1d, 0xd2, 0x94, 0xdd, 0xc4,
            0x76, 0x19, 0x39, 0x31, 0xf1, 0xad, 0xb5, 0x58, 0xf0, 0x93, 0x97, 0x32, 0x19, 0x2b, 0xd1,
            0xc0, 0xfd, 0x16, 0x8e, 0x4e];
        let id = SoftwareEd25519Identity::new(&seed);

        let public_key1 = id.get_public_key();
        let public_key2 = id.get_public_key();

        assert_eq!(public_key1, public_key2);
    }

    /*
    // TODO: This test fails for current version (0.2.36) of rust-crypto.
    // Uncomment it when problem is fixed. See also:
    // https://github.com/DaGenix/rust-crypto/issues/428

    #[test]
    fn test_rust_crypto_keypair_short_seed() {
        let seed: &[u8] = &[1,2,3,4,5];
        let (private_key, public_key) = keypair(seed);

        let message = b"This is my message!";
        let sig = signature(message, &private_key);
        assert!(verify(message, &public_key, &sig));

    }
    */

    #[test]
    fn test_rust_crypto_keypair_long_seed() {
        let seed: &[u8] = &[0x26, 0x27, 0xf6, 0x85, 0x97, 0x15, 0xad, 0x1d, 0xd2, 0x94, 0xdd, 0xc4,
            0x76, 0x19, 0x39, 0x31, 0xf1, 0xad, 0xb5, 0x58, 0xf0, 0x93, 0x97, 0x32, 0x19, 0x2b, 0xd1,
            0xc0, 0xfd, 0x16, 0x8e, 0x4e];
        let (private_key, public_key) = keypair(seed);

        let message = b"This is my message!";
        let sig = signature(message, &private_key);
        assert!(verify(message, &public_key, &sig));

    }

    #[test]
    fn test_sign_verify_self() {
        // let seed: &[u8] = &[1,2,3,4,5];
        let seed: &[u8] = &[0x26, 0x27, 0xf6, 0x85, 0x97, 0x15, 0xad, 0x1d, 0xd2, 0x94, 0xdd, 0xc4,
            0x76, 0x19, 0x39, 0x31, 0xf1, 0xad, 0xb5, 0x58, 0xf0, 0x93, 0x97, 0x32, 0x19, 0x2b, 0xd1,
            0xc0, 0xfd, 0x16, 0x8e, 0x4e];

        let id = SoftwareEd25519Identity::new(&seed);

        let message = b"This is a message";

        let signature = id.sign_message(message);
        let public_key = id.get_public_key();
        // println!("public_key = {:?}", public_key);
        // println!("signature = {:?}", signature);

        assert!(id.verify_signature(message, &public_key, &signature));

    }

    #[test]
    fn test_sign_verify_other() {
        let seed1: &[u8] = &[0x26, 0x27, 0xf6, 0x85, 0x97, 0x15, 0xad, 0x1d, 0xd2, 0x94, 0xdd, 0xc4,
            0x76, 0x19, 0x39, 0x31, 0xf1, 0xad, 0xb5, 0x58, 0xf0, 0x93, 0x97, 0x32, 0x19, 0x2b, 0xd1,
            0xc0, 0xfd, 0x16, 0x8e, 0x4e];
        let seed2: &[u8] = &[0x28, 0x27, 0xf6, 0x85, 0x97, 0x15, 0xad, 0x1d, 0xd2, 0x94, 0xdd, 0xc4,
            0x76, 0x19, 0x39, 0x31, 0xf1, 0xad, 0xb5, 0x58, 0xf0, 0x93, 0x97, 0x32, 0x19, 0x2b, 0xd1,
            0xc0, 0xfd, 0x16, 0x8e, 0x4e];
        let id1 = SoftwareEd25519Identity::new(&seed1);
        let id2 = SoftwareEd25519Identity::new(&seed2);

        let message = b"This is a message";
        let signature1 = id1.sign_message(message);

        let public_key1 = id1.get_public_key();
        assert!(id2.verify_signature(message, &public_key1, &signature1));

    }

    #[test]
    fn test_gen_shared_secret() {
        let seed1: &[u8] = &[0x26, 0x27, 0xf6, 0x85, 0x97, 0x15, 0xad, 0x1d, 0xd2, 0x94, 0xdd, 0xc4,
            0x76, 0x19, 0x39, 0x31, 0xf1, 0xad, 0xb5, 0x58, 0xf0, 0x93, 0x97, 0x32, 0x19, 0x2b, 0xd1,
            0xc0, 0xfd, 0x16, 0x8e, 0x4e];
        let seed2: &[u8] = &[0x28, 0x27, 0xf6, 0x85, 0x97, 0x15, 0xad, 0x1d, 0xd2, 0x94, 0xdd, 0xc4,
            0x76, 0x19, 0x39, 0x31, 0xf1, 0xad, 0xb5, 0x58, 0xf0, 0x93, 0x97, 0x32, 0x19, 0x2b, 0xd1,
            0xc0, 0xfd, 0x16, 0x8e, 0x4e];
        let id1 = SoftwareEd25519Identity::new(&seed1);
        let id2 = SoftwareEd25519Identity::new(&seed2);

        let ss12 = id1.gen_shared_secret(&id2.get_public_key());
        let ss21 = id2.gen_shared_secret(&id1.get_public_key());

        // Check that both sides get the exact same shared secret:
        assert_eq!(ss12, ss21);

        // Check determinism:
        let ss12_again = id1.gen_shared_secret(&id2.get_public_key());
        assert_eq!(ss12, ss12_again);
    }
}

