use std::convert::TryFrom;

use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};

use serde::{Deserialize, Serialize};

use capnp_conv::{CapnpConvError, ReadCapnp, WriteCapnp};

use common::b64_array::B64Array;
use common::define_fixed_bytes;

use crate::common_capnp;

#[macro_use]
mod serialize;

// use self::serialize::{type_capnp_serde128, type_capnp_serde256, type_capnp_serde512};

define_fixed_bytes!(HashResult, 32);
type_capnp_serde!(HashResult, common_capnp::hash_result, (x0, x1, x2, x3));

define_fixed_bytes!(Salt, 32);
type_capnp_serde!(Salt, common_capnp::salt, (x0, x1, x2, x3));

define_fixed_bytes!(DhPublicKey, 32);
type_capnp_serde!(DhPublicKey, common_capnp::dh_public_key, (x0, x1, x2, x3));

define_fixed_bytes!(PlainLock, 32);
type_capnp_serde!(PlainLock, common_capnp::plain_lock, (x0, x1, x2, x3));

define_fixed_bytes!(HashedLock, 32);
type_capnp_serde!(HashedLock, common_capnp::hashed_lock, (x0, x1, x2, x3));

define_fixed_bytes!(PublicKey, 32);
type_capnp_serde!(PublicKey, common_capnp::public_key, (x0, x1, x2, x3));

// PKCS8 key pair
define_fixed_bytes!(PrivateKey, 85);

define_fixed_bytes!(Signature, 64);
type_capnp_serde!(
    Signature,
    common_capnp::signature,
    (x0, x1, x2, x3, x4, x5, x6, x7)
);

// An invoice identifier
define_fixed_bytes!(InvoiceId, 32);
type_capnp_serde!(InvoiceId, common_capnp::invoice_id, (x0, x1, x2, x3));

define_fixed_bytes!(PaymentId, 16);
type_capnp_serde!(PaymentId, common_capnp::payment_id, (x0, x1));

define_fixed_bytes!(RandValue, 16);
type_capnp_serde!(RandValue, common_capnp::rand_value, (x0, x1));

// An Universally Unique Identifier (UUID).
define_fixed_bytes!(Uid, 16);
type_capnp_serde!(Uid, common_capnp::uid, (x0, x1));
