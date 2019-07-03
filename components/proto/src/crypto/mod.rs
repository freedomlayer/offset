use derive_more::From;
use serde::{Deserialize, Serialize};

use common::big_array::BigArray;
use common::define_fixed_bytes;

pub const HASH_RESULT_LEN: usize = 32;
define_fixed_bytes!(HashResult, HASH_RESULT_LEN);

pub const SALT_LEN: usize = 32;
pub const DH_PUBLIC_KEY_LEN: usize = 32;
pub const SHARED_SECRET_LEN: usize = 32;

define_fixed_bytes!(Salt, SALT_LEN);
define_fixed_bytes!(DhPublicKey, DH_PUBLIC_KEY_LEN);

pub const PLAIN_LOCK_LEN: usize = 32;
pub const HASHED_LOCK_LEN: usize = 32;

define_fixed_bytes!(PlainLock, PLAIN_LOCK_LEN);
define_fixed_bytes!(HashedLock, HASHED_LOCK_LEN);

pub const PUBLIC_KEY_LEN: usize = 32;
pub const SIGNATURE_LEN: usize = 64;
pub const PRIVATE_KEY_LEN: usize = 85;

define_fixed_bytes!(PublicKey, PUBLIC_KEY_LEN);

/// PKCS8 key pair
#[derive(Clone, Serialize, Deserialize, From)]
pub struct PrivateKey(#[serde(with = "BigArray")] [u8; PRIVATE_KEY_LEN]);

#[derive(Clone, Serialize, Deserialize, From)]
pub struct Signature(#[serde(with = "BigArray")] [u8; SIGNATURE_LEN]);

pub const INVOICE_ID_LEN: usize = 32;

// An invoice identifier
define_fixed_bytes!(InvoiceId, INVOICE_ID_LEN);

pub const PAYMENT_ID_LEN: usize = 16;

define_fixed_bytes!(PaymentId, PAYMENT_ID_LEN);

pub const RAND_VALUE_LEN: usize = 16;

define_fixed_bytes!(RandValue, RAND_VALUE_LEN);

pub const UID_LEN: usize = 16;

// An Universally Unique Identifier (UUID).
define_fixed_bytes!(Uid, UID_LEN);

// TODO: Fix define_fixed_bytes to handle those cases too:

// ==================== Convenience for Signature ====================
// Because SIGNATURE_LEN > 32, these trait can't be derived automatically.
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

// ==================== Convenience for PrivateKey ====================
// Because PRIVATE_KEY_LEN > 32, these trait can't be derived automatically.
// ===================================================================

impl AsRef<[u8]> for PrivateKey {
    #[inline]
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl ::std::ops::Deref for PrivateKey {
    type Target = [u8];
    #[inline]
    fn deref(&self) -> &[u8] {
        &self.0
    }
}

impl ::std::ops::DerefMut for PrivateKey {
    #[inline]
    fn deref_mut(&mut self) -> &mut [u8] {
        &mut self.0
    }
}

impl<'a> ::std::convert::From<&'a [u8; PRIVATE_KEY_LEN]> for PrivateKey {
    #[inline]
    fn from(src: &'a [u8; PRIVATE_KEY_LEN]) -> PrivateKey {
        let mut inner = [0x00u8; PRIVATE_KEY_LEN];
        inner.copy_from_slice(&src[..PRIVATE_KEY_LEN]);
        PrivateKey(inner)
    }
}

impl<'a> ::std::convert::TryFrom<&'a [u8]> for PrivateKey {
    type Error = ();

    #[inline]
    fn try_from(src: &'a [u8]) -> Result<PrivateKey, ()> {
        if src.len() < PRIVATE_KEY_LEN {
            Err(())
        } else {
            let mut inner = [0x00u8; PRIVATE_KEY_LEN];
            inner.copy_from_slice(&src[..PRIVATE_KEY_LEN]);
            Ok(PrivateKey(inner))
        }
    }
}

impl<'a> ::std::convert::TryFrom<&'a ::bytes::Bytes> for PrivateKey {
    type Error = ();

    #[inline]
    fn try_from(src: &'a ::bytes::Bytes) -> Result<PrivateKey, ()> {
        if src.len() < PRIVATE_KEY_LEN {
            Err(())
        } else {
            let mut inner = [0x00u8; PRIVATE_KEY_LEN];
            inner.copy_from_slice(&src[..PRIVATE_KEY_LEN]);
            Ok(PrivateKey(inner))
        }
    }
}

impl PartialEq for PrivateKey {
    #[inline]
    fn eq(&self, other: &PrivateKey) -> bool {
        for i in 0..PRIVATE_KEY_LEN {
            if self.0[i] != other.0[i] {
                return false;
            }
        }
        true
    }
}

impl Eq for PrivateKey {}

impl ::std::fmt::Debug for PrivateKey {
    fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
        ::std::fmt::Debug::fmt(&self.0[..], f)
    }
}
