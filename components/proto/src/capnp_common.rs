use std::io;
use std::convert::TryFrom;
use byteorder::{BigEndian, WriteBytesExt, ReadBytesExt};

use common_capnp::{custom_u_int128, custom_u_int256, custom_u_int512,
                    public_key, invoice_id, hash, dh_public_key, salt, signature,
                    rand_nonce};

use crate::funder::messages::InvoiceId;

use crypto::identity::{PublicKey, Signature};
use crypto::dh::{DhPublicKey, Salt};
use crypto::hash::HashResult;
use crypto::crypto_rand::RandValue;


/// Read the underlying bytes from given `CustomUInt128` reader.
fn read_custom_u_int128(from: &custom_u_int128::Reader) -> Vec<u8> {
    let mut vec = Vec::new();
    vec.write_u64::<BigEndian>(from.get_x0()).unwrap();
    vec.write_u64::<BigEndian>(from.get_x1()).unwrap();
    vec
}

/// Fill the components of `CustomUInt128` from given bytes.
fn write_custom_u_int128(from: impl AsRef<[u8]>, to: &mut custom_u_int128::Builder) {
    let mut reader = io::Cursor::new(from.as_ref());
    to.set_x0(reader.read_u64::<BigEndian>().unwrap());
    to.set_x1(reader.read_u64::<BigEndian>().unwrap());
}

/// Read the underlying bytes from given `CustomUInt256` reader.
fn read_custom_u_int256(from: &custom_u_int256::Reader) -> Vec<u8> {
    let mut vec = Vec::new();
    vec.write_u64::<BigEndian>(from.get_x0()).unwrap();
    vec.write_u64::<BigEndian>(from.get_x1()).unwrap();
    vec.write_u64::<BigEndian>(from.get_x2()).unwrap();
    vec.write_u64::<BigEndian>(from.get_x3()).unwrap();
    vec
}

/// Fill the components of `CustomUInt256` from given bytes.
fn write_custom_u_int256(from: impl AsRef<[u8]>, to: &mut custom_u_int256::Builder) {
    let mut reader = io::Cursor::new(from.as_ref());
    to.set_x0(reader.read_u64::<BigEndian>().unwrap());
    to.set_x1(reader.read_u64::<BigEndian>().unwrap());
    to.set_x2(reader.read_u64::<BigEndian>().unwrap());
    to.set_x3(reader.read_u64::<BigEndian>().unwrap());
}


/// Read the underlying bytes from given `CustomUInt512` reader.
fn read_custom_u_int512(from: &custom_u_int512::Reader) -> Vec<u8> {
    let mut vec = Vec::new();
    vec.write_u64::<BigEndian>(from.get_x0()).unwrap();
    vec.write_u64::<BigEndian>(from.get_x1()).unwrap();
    vec.write_u64::<BigEndian>(from.get_x2()).unwrap();
    vec.write_u64::<BigEndian>(from.get_x3()).unwrap();
    vec.write_u64::<BigEndian>(from.get_x4()).unwrap();
    vec.write_u64::<BigEndian>(from.get_x5()).unwrap();
    vec.write_u64::<BigEndian>(from.get_x6()).unwrap();
    vec.write_u64::<BigEndian>(from.get_x7()).unwrap();
    vec
}

/// Fill the components of `CustomUInt512` from given bytes.
fn write_custom_u_int512(from: impl AsRef<[u8]>, to: &mut custom_u_int512::Builder) {
    let mut reader = io::Cursor::new(from.as_ref());
    to.set_x0(reader.read_u64::<BigEndian>().unwrap());
    to.set_x1(reader.read_u64::<BigEndian>().unwrap());
    to.set_x2(reader.read_u64::<BigEndian>().unwrap());
    to.set_x3(reader.read_u64::<BigEndian>().unwrap());
    to.set_x4(reader.read_u64::<BigEndian>().unwrap());
    to.set_x5(reader.read_u64::<BigEndian>().unwrap());
    to.set_x6(reader.read_u64::<BigEndian>().unwrap());
    to.set_x7(reader.read_u64::<BigEndian>().unwrap());
}

/// Define read and write functions for basic types
macro_rules! type_capnp_serde {
    ($capnp_type:ident, $native_type:ident, $read_func:ident, $write_func:ident, $inner_read_func:ident, $inner_write_func:ident) => {

        pub fn $read_func(from: &$capnp_type::Reader) -> Result<$native_type, capnp::Error>  {
            let inner = from.get_inner()?;
            let data_bytes = &$inner_read_func(&inner);
            Ok($native_type::try_from(&data_bytes[..]).unwrap())
        }

        pub fn $write_func(from: &$native_type, to: &mut $capnp_type::Builder) {
            let mut inner = to.reborrow().get_inner().unwrap();
            $inner_write_func(from, &mut inner);
        }
    }
}

// 128 bits:
type_capnp_serde!(rand_nonce, RandValue, read_rand_nonce, write_rand_nonce, read_custom_u_int128, write_custom_u_int128);

// 256 bits:
type_capnp_serde!(public_key, PublicKey, read_public_key, write_public_key, read_custom_u_int256, write_custom_u_int256);
type_capnp_serde!(dh_public_key, DhPublicKey, read_dh_public_key, write_dh_public_key, read_custom_u_int256, write_custom_u_int256);
type_capnp_serde!(salt, Salt, read_salt, write_salt, read_custom_u_int256, write_custom_u_int256);
type_capnp_serde!(hash, HashResult, read_hash, write_hash, read_custom_u_int256, write_custom_u_int256);
type_capnp_serde!(invoice_id, InvoiceId, read_invoice_id, write_invoice_id, read_custom_u_int256, write_custom_u_int256);

// 512 bits:
type_capnp_serde!(signature, Signature, read_signature, write_signature, read_custom_u_int512, write_custom_u_int512);



