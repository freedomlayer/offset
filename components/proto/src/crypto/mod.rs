// use std::convert::TryFrom;

// use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};

use serde::{Deserialize, Serialize};

// use capnp_conv::{CapnpConvError, ReadCapnp, WriteCapnp};

use common::big_array::BigArray;
use common::define_fixed_bytes;

// use crate::common_capnp;

// #[macro_use]
// mod serialize;

define_fixed_bytes!(HmacResult, 32);

define_fixed_bytes!(HmacKey, 32);

define_fixed_bytes!(HashResult, 32);

define_fixed_bytes!(Salt, 32);

define_fixed_bytes!(DhPublicKey, 32);

define_fixed_bytes!(PlainLock, 32);

define_fixed_bytes!(HashedLock, 32);

define_fixed_bytes!(PublicKey, 32);

// PKCS8 key pair
define_fixed_bytes!(PrivateKey, 32);

define_fixed_bytes!(Signature, 64);

// An invoice identifier
define_fixed_bytes!(InvoiceId, 32);

define_fixed_bytes!(PaymentId, 16);

define_fixed_bytes!(NodePort, 16);

define_fixed_bytes!(RandValue, 16);

// An Universally Unique Identifier (UUID).
define_fixed_bytes!(Uid, 16);
