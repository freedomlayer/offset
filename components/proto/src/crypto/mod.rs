use std::convert::TryFrom;

use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};

use serde::{Deserialize, Serialize};

use capnp_conv::{CapnpConvError, ReadCapnp, WriteCapnp};

use common::big_array::BigArray;
use common::define_fixed_bytes;

use crate::common_capnp;

#[macro_use]
mod serialize;

// use self::serialize::{type_capnp_serde128, type_capnp_serde256, type_capnp_serde512};

pub const HASH_RESULT_LEN: usize = 32;
define_fixed_bytes!(HashResult, HASH_RESULT_LEN);
type_capnp_serde!(HashResult, common_capnp::hash_result, (x0, x1, x2, x3));

pub const SALT_LEN: usize = 32;
pub const DH_PUBLIC_KEY_LEN: usize = 32;
pub const SHARED_SECRET_LEN: usize = 32;

define_fixed_bytes!(Salt, SALT_LEN);
type_capnp_serde!(Salt, common_capnp::salt, (x0, x1, x2, x3));

define_fixed_bytes!(DhPublicKey, DH_PUBLIC_KEY_LEN);
type_capnp_serde!(DhPublicKey, common_capnp::dh_public_key, (x0, x1, x2, x3));

pub const PLAIN_LOCK_LEN: usize = 32;
pub const HASHED_LOCK_LEN: usize = 32;

define_fixed_bytes!(PlainLock, PLAIN_LOCK_LEN);
type_capnp_serde!(PlainLock, common_capnp::plain_lock, (x0, x1, x2, x3));

define_fixed_bytes!(HashedLock, HASHED_LOCK_LEN);
type_capnp_serde!(HashedLock, common_capnp::hashed_lock, (x0, x1, x2, x3));

pub const PUBLIC_KEY_LEN: usize = 32;
pub const SIGNATURE_LEN: usize = 64;
pub const PRIVATE_KEY_LEN: usize = 85;

define_fixed_bytes!(PublicKey, PUBLIC_KEY_LEN);
type_capnp_serde!(PublicKey, common_capnp::public_key, (x0, x1, x2, x3));

// PKCS8 key pair
define_fixed_bytes!(PrivateKey, PRIVATE_KEY_LEN);

define_fixed_bytes!(Signature, SIGNATURE_LEN);
type_capnp_serde!(
    Signature,
    common_capnp::signature,
    (x0, x1, x2, x3, x4, x5, x6, x7)
);

pub const INVOICE_ID_LEN: usize = 32;

// An invoice identifier
define_fixed_bytes!(InvoiceId, INVOICE_ID_LEN);
type_capnp_serde!(InvoiceId, common_capnp::invoice_id, (x0, x1, x2, x3));

pub const PAYMENT_ID_LEN: usize = 16;

define_fixed_bytes!(PaymentId, PAYMENT_ID_LEN);
type_capnp_serde!(PaymentId, common_capnp::payment_id, (x0, x1));

pub const RAND_VALUE_LEN: usize = 16;

define_fixed_bytes!(RandValue, RAND_VALUE_LEN);
type_capnp_serde!(RandValue, common_capnp::rand_value, (x0, x1));

pub const UID_LEN: usize = 16;

// An Universally Unique Identifier (UUID).
define_fixed_bytes!(Uid, UID_LEN);
type_capnp_serde!(Uid, common_capnp::uid, (x0, x1));
