use serde::{Deserialize, Serialize};

use common::big_array::BigArray;
use common::define_fixed_bytes;

mod serialize;

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
// PKCS8 key pair
define_fixed_bytes!(PrivateKey, PRIVATE_KEY_LEN);
define_fixed_bytes!(Signature, SIGNATURE_LEN);

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
