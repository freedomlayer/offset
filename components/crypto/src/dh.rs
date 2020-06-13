use proto::crypto::{DhPublicKey, Salt};

use hkdf::Hkdf;
use sha2::Sha512Trunc256;

use crate::error::CryptoError;
use crate::rand::CryptoRandom;
use crate::sym_encrypt::{SymmetricKey, SYMMETRIC_KEY_LEN};

pub const SHARED_SECRET_LEN: usize = 32;

pub struct DhPrivateKey(x25519_dalek::EphemeralSecret);

impl DhPrivateKey {
    /// Create a new ephemeral private key.
    pub fn new<R: CryptoRandom>(rng: &mut R) -> Result<DhPrivateKey, CryptoError> {
        Ok(DhPrivateKey(x25519_dalek::EphemeralSecret::new(rng)))
    }

    /// Compute public key from our private key.
    /// The public key will be sent to remote side.
    pub fn compute_public_key(&self) -> Result<DhPublicKey, CryptoError> {
        let mut public_key = DhPublicKey::default();
        Ok(DhPublicKey::from(
            x25519_dalek::PublicKey::from(&self.0).as_bytes(),
        ))
    }

    /// Derive a symmetric key from our private key and remote's public key.
    pub fn derive_symmetric_key(
        self,
        remote_public_key: DhPublicKey,
        send_salt: Salt,
        recv_salt: Salt,
    ) -> Result<(SymmetricKey, SymmetricKey), CryptoError> {
        let dalek_remote_public_key =
            x25519_dalek::PublicKey::from(remote_public_key.as_array_ref().clone());
        let shared_secret = self.0.diffie_hellman(&dalek_remote_public_key);

        let send_h = Hkdf::<Sha512Trunc256>::new(Some(&send_salt), shared_secret.as_bytes());
        let recv_h = Hkdf::<Sha512Trunc256>::new(Some(&recv_salt), shared_secret.as_bytes());

        let mut send_key_raw = [0u8; SymmetricKey::len()];
        let mut recv_key_raw = [0u8; SymmetricKey::len()];

        let empty_info: [u8; 0] = [];

        send_h
            .expand(&empty_info, &mut send_key_raw)
            .map_err(|_| CryptoError)?;
        recv_h
            .expand(&empty_info, &mut recv_key_raw)
            .map_err(|_| CryptoError)?;

        Ok((
            SymmetricKey::from(&send_key_raw),
            SymmetricKey::from(&recv_key_raw),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_utils::DummyRandom;
    use super::*;

    use crate::rand::RandGen;

    #[test]
    fn test_new_salt() {
        let mut rng = DummyRandom::new(&[1, 2, 3, 4, 6]);
        let salt1 = Salt::rand_gen(&mut rng);
        let salt2 = Salt::rand_gen(&mut rng);

        assert_ne!(salt1, salt2);
    }

    #[test]
    fn test_derive_symmetric_key() {
        let mut rng = DummyRandom::new(&[1, 2, 3, 4, 5]);
        let dh_private_a = DhPrivateKey::new(&mut rng).unwrap();
        let dh_private_b = DhPrivateKey::new(&mut rng).unwrap();

        let public_key_a = dh_private_a.compute_public_key().unwrap();
        let public_key_b = dh_private_b.compute_public_key().unwrap();

        let salt_a = Salt::rand_gen(&mut rng);
        let salt_b = Salt::rand_gen(&mut rng);

        // Each side derives the symmetric key from the remote's public key
        // and the salt:
        let (send_key_a, recv_key_a) = dh_private_a
            .derive_symmetric_key(public_key_b, salt_a.clone(), salt_b.clone())
            .unwrap();

        let (send_key_b, recv_key_b) = dh_private_b
            .derive_symmetric_key(public_key_a, salt_b, salt_a)
            .unwrap();

        // Both sides should get the same derived symmetric key:
        assert_eq!(send_key_a, recv_key_b);
        assert_eq!(send_key_b, recv_key_a)
    }
}
