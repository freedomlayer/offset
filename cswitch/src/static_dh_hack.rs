extern crate ring;

use std::mem;

use self::ring::{signature, agreement};

const ELEM_MAX_BITS: usize = 384;
const ELEM_MAX_BYTES: usize = (ELEM_MAX_BITS + 7) / 8;
const SCALAR_MAX_BYTES: usize = ELEM_MAX_BYTES;

pub struct CustomPrivateKey {
    bytes: [u8; SCALAR_MAX_BYTES],
}

struct CustomEphemeralPrivateKey {
    private_key: CustomPrivateKey,
    alg: &'static agreement::Algorithm,
}

pub const SCALAR_LEN: usize = 32;

struct CustomEd25519KeyPair {
    private_scalar: [u8; SCALAR_LEN],
    // We don't care about the rest of the fields here...
}


/// Take a private key from the inside of Ed25519KeyPair, and create an EphemeralPrivateKey with
/// that private key.
pub fn key_pair_to_ephemeral_private_key(key_pair: &signature::Ed25519KeyPair) -> agreement::EphemeralPrivateKey {
    let ced25519kp = unsafe {
        mem::transmute::<&signature::Ed25519KeyPair, &CustomEd25519KeyPair> (key_pair)
    };

    let mut cepk = CustomEphemeralPrivateKey {
        private_key: CustomPrivateKey { bytes: [0; SCALAR_MAX_BYTES] },
        alg: &agreement::X25519,
    };

    // Copy the private key:
    for i in 0 .. ced25519kp.private_scalar.len() {
        cepk.private_key.bytes[i] = ced25519kp.private_scalar[i];
    }

    unsafe {
        mem::transmute::<CustomEphemeralPrivateKey, agreement::EphemeralPrivateKey> (cepk)
    }
}


