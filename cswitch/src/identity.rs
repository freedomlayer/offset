extern crate ring;
extern crate untrusted;
extern crate crypto;

use std::fmt;

use self::crypto::ed25519::{signature, verify, keypair, exchange};
use self::ring::hkdf::extract_and_expand;
use self::ring::hmac::SigningKey;

// use self::ring::{signature, agreement};
// use ::static_dh_hack::key_pair_to_ephemeral_private_key;

const PUBLIC_KEY_LEN: usize = 32;
const SIGNATURE_LEN: usize = 64;
pub const SYMMETRIC_KEY_LEN: usize = 32;
const SALT_LEN: usize = 32;

#[derive(Debug, PartialEq, Eq, Hash, Clone)]
pub struct PublicKey([u8; PUBLIC_KEY_LEN]);

#[derive(Debug, PartialEq, Eq, Hash, Clone)]
pub struct Salt([u8; SALT_LEN]);

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
pub struct SymmetricKey(pub [u8; SYMMETRIC_KEY_LEN]);


/// A generic interface for signing and verifying messages.
pub trait Identity {
    /// Verify a signature of a given message
    fn verify_signature(&self, message: &[u8], 
                        public_key: &PublicKey, signature: &Signature) -> bool;
    /// Create a signature for a given message using private key.
    fn sign_message(&self, message: &[u8]) -> Signature;
    /// Get our public identity
    fn get_public_key(&self) -> PublicKey;
    /*
    /// Create a shared secret from our identity,
    /// the public key of a remote user and a salt.
    fn gen_symmetric_key(&self, public_key: &PublicKey, salt: &Salt) -> SymmetricKey;
    */
}

/*

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



impl Identity for SoftwareEd25519Identity {
    fn verify_signature(&self, message: &[u8], 
                        public_key: &PublicKey, signature: &Signature) -> bool {

        let public_key = untrusted::Input::from(&public_key.0);
        let message = untrusted::Input::from(message);
        let signature = untrusted::Input::from(&signature.0);
        match signature::verify(&signature::ED25519, public_key, message, signature) {
            Ok(()) => true,
            Err(ring::error::Unspecified) => false,
        }
    }

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

    fn gen_shared_secret(&self, public_key: &PublicKey) -> SharedSecret {
        // This is an (unsafe) hack, because of current limitations of the ring crate:
        let private_key = key_pair_to_ephemeral_private_key(&self.key_pair);

        // Sanity check for the obtained private key:
        let mut computed_public_key = PublicKey([0; 32]);
        private_key.compute_public_key(&mut computed_public_key.0).unwrap();
        println!("computed_public_key = {:?}", computed_public_key);
        println!("key_pair public key = {:?}", self.key_pair.public_key_bytes());
        assert_eq!(self.key_pair.public_key_bytes(), &computed_public_key.0);

        let public_key = untrusted::Input::from(&public_key.0);

        let key_material_res = agreement::agree_ephemeral(private_key, 
                    &agreement::X25519, public_key, 
                    ring::error::Unspecified ,|key_material| {
                        assert_eq!(key_material.len(), SHARED_SECRET_LEN);
                        let mut shared_secret_array = [0; SHARED_SECRET_LEN];
                        shared_secret_array.clone_from_slice(key_material);
                        Ok(SharedSecret(shared_secret_array))
                    });

        match key_material_res {
            Ok(key_material) => key_material,
            _ => unreachable!(),
        }
    }
}
*/


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

    /*
    fn gen_symmetric_key(&self, public_key: &PublicKey, salt: &Salt) -> SymmetricKey {
        // Obtain raw secret from static diffie hellman:
        let raw_secret = exchange(&public_key.0, &self.private_key);
        // Add the salt to the raw_secret using hkdf:
        let skey_salt = SigningKey::new(&ring::digest::SHA512_256, &salt.0);
        let info: [u8; 0] = [];
        let mut out = [0_u8; SYMMETRIC_KEY_LEN];
        extract_and_expand(&skey_salt, &raw_secret, &info, &mut out);
        SymmetricKey(out)
    }
    */

}


#[cfg(test)]
mod tests {
    extern crate rand;
    use super::*;
    // use ::test_utils::DummyRandom;

    use self::rand::{Rng, StdRng};


    #[test]
    fn test_get_public_key_sanity() {

        /*
        let secure_rand = DummyRandom::new(&[1,2,3,4,5]);
        let pkcs8 = signature::Ed25519KeyPair::generate_pkcs8(&secure_rand).unwrap();
        let id = SoftwareEd25519Identity::from_pkcs8(&pkcs8).unwrap();
        */

        let rng_seed: &[_] = &[1,2,3,4,5];
        let mut rng: StdRng = rand::SeedableRng::from_seed(rng_seed);
        let mut identity_seed = [0; 32];
        rng.fill_bytes(&mut identity_seed);
        let id = SoftwareEd25519Identity::new(&identity_seed);

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
        /*
        let secure_rand = DummyRandom::new(&[1,2,3,4,5]);
        let pkcs8 = signature::Ed25519KeyPair::generate_pkcs8(&secure_rand).unwrap();
        let id = SoftwareEd25519Identity::from_pkcs8(&pkcs8).unwrap();
        */

        let rng_seed: &[_] = &[1,2,3,4,5];
        let mut rng: StdRng = rand::SeedableRng::from_seed(rng_seed);
        let mut identity_seed = [0; 32];
        rng.fill_bytes(&mut identity_seed);
        let id = SoftwareEd25519Identity::new(&identity_seed);

        let message = b"This is a message";

        let signature = id.sign_message(message);
        let public_key = id.get_public_key();
        // println!("public_key = {:?}", public_key);
        // println!("signature = {:?}", signature);

        assert!(id.verify_signature(message, &public_key, &signature));

    }

    #[test]
    fn test_sign_verify_other() {
        /*
        let secure_rand = DummyRandom::new(&[1,2,3,4,5]);
        let pkcs8 = signature::Ed25519KeyPair::generate_pkcs8(&secure_rand).unwrap();
        let id1 = SoftwareEd25519Identity::from_pkcs8(&pkcs8).unwrap();

        let secure_rand = DummyRandom::new(&[1,2,3,4,5,6]);
        let pkcs8 = signature::Ed25519KeyPair::generate_pkcs8(&secure_rand).unwrap();
        let id2 = SoftwareEd25519Identity::from_pkcs8(&pkcs8).unwrap();
        */

        let rng_seed: &[_] = &[1,2,3,4,5];
        let mut rng: StdRng = rand::SeedableRng::from_seed(rng_seed);
        let mut identity_seed = [0; 32];
        rng.fill_bytes(&mut identity_seed);
        let id1 = SoftwareEd25519Identity::new(&identity_seed);
        rng.fill_bytes(&mut identity_seed);
        let id2 = SoftwareEd25519Identity::new(&identity_seed);

        let message = b"This is a message";
        let signature1 = id1.sign_message(message);

        let public_key1 = id1.get_public_key();
        assert!(id2.verify_signature(message, &public_key1, &signature1));

    }

    /*
    #[test]
    fn test_gen_symmetric_key() {
        /*

        // Implementation with ring:

        let secure_rand = DummyRandom::new(&[1,2,3,4,5]);
        let pkcs8 = signature::Ed25519KeyPair::generate_pkcs8(&secure_rand).unwrap();
        let id1 = SoftwareEd25519Identity::from_pkcs8(&pkcs8).unwrap();

        let secure_rand = DummyRandom::new(&[1,2,3,4,5,6]);
        let pkcs8 = signature::Ed25519KeyPair::generate_pkcs8(&secure_rand).unwrap();
        let id2 = SoftwareEd25519Identity::from_pkcs8(&pkcs8).unwrap();
        */

        let rng_seed: &[_] = &[1,2,3,4,5];
        let mut rng: StdRng = rand::SeedableRng::from_seed(rng_seed);
        let mut identity_seed = [0; 32];
        rng.fill_bytes(&mut identity_seed);
        let id1 = SoftwareEd25519Identity::new(&identity_seed);
        rng.fill_bytes(&mut identity_seed);
        let id2 = SoftwareEd25519Identity::new(&identity_seed);

        let salt = Salt([1_u8; 32]);

        let ss12 = id1.gen_symmetric_key(&id2.get_public_key(), &salt);
        let ss21 = id2.gen_symmetric_key(&id1.get_public_key(), &salt);

        // Check that both sides get the exact same shared secret:
        assert_eq!(ss12, ss21);

        // Check determinism:
        let ss12_again = id1.gen_symmetric_key(&id2.get_public_key(), &salt);
        assert_eq!(ss12, ss12_again);

        // Different salt should yield different results:
        let other_salt = Salt([2_u8; 32]);
        let ss12_other_salt = id1.gen_symmetric_key(&id2.get_public_key(), &other_salt);

        assert_ne!(ss12, ss12_other_salt);
    }
    */
}

