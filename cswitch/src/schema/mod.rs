//! The schema module.
//!
//! # Introduction
//!
//! The module is used to encode/decode data for interchanging between
//! `CSwitch` nodes.
//!
//! Currently, we use [`capnp`][capnp] as the underlying protocol.
//!
//! # Data Format
//!
//! ## Custom Underlying Types
//!
//! We have some custom underlying types in the low-level, they are:
//!
//! - `CustomUInt128`: A custom made 128 bit data structure;
//! - `CustomUInt256`: A custom made 256 bit data structure;
//! - `CustomUInt512`: A custom made 512 bit data structure;
//!
//! We use them widely in communicating, the following is the mapping:
//!
//! - `Salt: CustomUInt256`
//! - `Signature: CustomUInt512`
//! - `RandValue: CustomUInt128`
//! - `PublicKey: CustomUInt256`
//! - `DhPublicKey: CustomUInt256`
//! - `IndexingProviderId: CustomUInt128`
//! - `IndexingProviderStateHash: CustomUInt256`
//!
//! ## Channeler
//!
//! TODO:
//!
//! ## Indexer
//!
//! TODO:
//!
//! [capnp]: https://capnproto.org

// #![deny(warnings)]

use std::io;

use bytes::{BigEndian, Buf, BufMut, Bytes, BytesMut};
use capnp::struct_list;

use crypto::rand_values::RandValue;
use crypto::dh::{DhPublicKey, Salt};
use crypto::identity::{PublicKey, Signature};

use indexer::types::{IndexingProviderId, IndexingProviderStateHash};

const CUSTOM_UINT128_LEN: usize = 16;
const CUSTOM_UINT256_LEN: usize = 32;
const CUSTOM_UINT512_LEN: usize = 64;

/// Include the auto-generated schema files.
macro_rules! include_schema {
    ($( $name:ident, $path:expr );*) => {
        $(
            // Allow clippy's `Warn` lint group
            #[allow(unused, clippy)]
            pub mod $name {
                include!(concat!(env!("OUT_DIR"), "/schema/", $path, ".rs"));
            }
        )*
    };
}

include_schema! {
    common_capnp,    "common_capnp";
    indexer_capnp,   "indexer_capnp";
    channeler_capnp, "channeler_capnp"
}

use common_capnp::{custom_u_int128, custom_u_int256, custom_u_int512};

pub trait Schema<'a>: Sized {
    type Reader: 'a;
    type Writer: 'a;

    fn decode(buffer: Bytes) -> Result<Self, SchemaError>;
    fn encode(&self) -> Result<Bytes, SchemaError>;
    fn read(from: &Self::Reader) -> Result<Self, SchemaError>;
    fn write(&self, to: &mut Self::Writer) -> Result<(), SchemaError>;
}

macro_rules! inject_default_impl {
    () => {
        fn decode(buffer: Bytes) -> Result<Self, SchemaError> {
            let mut buffer = io::Cursor::new(buffer);

            let reader = serialize_packed::read_message(
                &mut buffer,
                ::capnp::message::ReaderOptions::new()
            )?;

            Self::read(&reader.get_root()?)
        }

        fn encode(&self) -> Result<Bytes, SchemaError> {
            let mut builder = ::capnp::message::Builder::new_default();

            match self.write(&mut builder.init_root())? {
                () => {
                    let mut serialized_msg = Vec::new();

                    serialize_packed::write_message(
                        &mut serialized_msg,
                        &builder
                    )?;

                    Ok(Bytes::from(serialized_msg))
                }
            }
        }
    };
}

#[cfg(test)]
macro_rules! test_encode_decode {
        ($type: ident, $in: ident) => {
            let msg = $in.encode().unwrap();
            let out = $type::decode(msg).unwrap();
            assert!($in == out);
        };
}

pub mod channeler;
pub mod indexer;
// pub mod networker;

#[derive(Debug)]
pub enum SchemaError {
    Io(io::Error),
    Capnp(::capnp::Error),
    Invalid,
    NotInSchema,
}

impl From<io::Error> for SchemaError {
    #[inline]
    fn from(e: io::Error) -> SchemaError {
        SchemaError::Io(e)
    }
}

impl From<::capnp::Error> for SchemaError {
    #[inline]
    fn from(e: ::capnp::Error) -> SchemaError {
        SchemaError::Capnp(e)
    }
}

impl From<::capnp::NotInSchema> for SchemaError {
    #[inline]
    fn from(_: ::capnp::NotInSchema) -> SchemaError {
        SchemaError::NotInSchema
    }
}

/// Read the underlying bytes from given `CustomUInt128` reader.
#[inline]
pub fn read_custom_u_int128(from: &custom_u_int128::Reader) -> Result<Bytes, SchemaError> {
    let mut buffer = BytesMut::with_capacity(CUSTOM_UINT128_LEN);

    buffer.put_u64::<BigEndian>(from.get_x0());
    buffer.put_u64::<BigEndian>(from.get_x1());

    Ok(buffer.freeze())
}

/// Fill the components of `CustomUInt128` from given bytes.
///
/// # Panics
///
/// This function panics if there is not enough remaining data in `from`.
#[inline]
pub fn write_custom_u_int128<T: AsRef<[u8]>>(
    from: &T,
    to: &mut custom_u_int128::Builder,
) -> Result<(), SchemaError> {
    let mut reader = io::Cursor::new(from.as_ref());

    to.set_x0(reader.get_u64::<BigEndian>());
    to.set_x1(reader.get_u64::<BigEndian>());

    Ok(())
}

/// Read the underlying bytes from given `CustomUInt256` reader.
#[inline]
pub fn read_custom_u_int256(from: &custom_u_int256::Reader) -> Result<Bytes, SchemaError> {
    let mut buffer = BytesMut::with_capacity(CUSTOM_UINT256_LEN);

    buffer.put_u64::<BigEndian>(from.get_x0());
    buffer.put_u64::<BigEndian>(from.get_x1());
    buffer.put_u64::<BigEndian>(from.get_x2());
    buffer.put_u64::<BigEndian>(from.get_x3());

    Ok(buffer.freeze())
}

/// Fill the components of `CustomUInt256` from given bytes.
///
/// # Panics
///
/// This function panics if there is not enough remaining data in `from`.
#[inline]
pub fn write_custom_u_int256<T: AsRef<[u8]>>(
    from: &T,
    to: &mut custom_u_int256::Builder,
) -> Result<(), SchemaError> {
    let mut reader = io::Cursor::new(from.as_ref());

    to.set_x0(reader.get_u64::<BigEndian>());
    to.set_x1(reader.get_u64::<BigEndian>());
    to.set_x2(reader.get_u64::<BigEndian>());
    to.set_x3(reader.get_u64::<BigEndian>());

    Ok(())
}

/// Read the underlying bytes from given `CustomUInt512` reader.
#[inline]
pub fn read_custom_u_int512(from: &custom_u_int512::Reader) -> Result<Bytes, SchemaError> {
    let mut buffer = BytesMut::with_capacity(CUSTOM_UINT512_LEN);

    buffer.put_u64::<BigEndian>(from.get_x0());
    buffer.put_u64::<BigEndian>(from.get_x1());
    buffer.put_u64::<BigEndian>(from.get_x2());
    buffer.put_u64::<BigEndian>(from.get_x3());
    buffer.put_u64::<BigEndian>(from.get_x4());
    buffer.put_u64::<BigEndian>(from.get_x5());
    buffer.put_u64::<BigEndian>(from.get_x6());
    buffer.put_u64::<BigEndian>(from.get_x7());

    Ok(buffer.freeze())
}

/// Fill the components of `CustomUInt512` from given bytes.
///
/// # Panics
///
/// This function panics if there is not enough remaining data in `from`.
#[inline]
pub fn write_custom_u_int512<T: AsRef<[u8]>>(
    from: &T,
    to: &mut custom_u_int512::Builder,
) -> Result<(), SchemaError> {
    let mut reader = io::Cursor::new(from.as_ref());

    to.set_x0(reader.get_u64::<BigEndian>());
    to.set_x1(reader.get_u64::<BigEndian>());
    to.set_x2(reader.get_u64::<BigEndian>());
    to.set_x3(reader.get_u64::<BigEndian>());
    to.set_x4(reader.get_u64::<BigEndian>());
    to.set_x5(reader.get_u64::<BigEndian>());
    to.set_x6(reader.get_u64::<BigEndian>());
    to.set_x7(reader.get_u64::<BigEndian>());

    Ok(())
}

// ======= r/w implementation =======
// TODO: Can we use macro to generate these code automatically?

#[inline]
pub fn read_public_key(from: &custom_u_int256::Reader) -> Result<PublicKey, SchemaError> {
    PublicKey::from_bytes(&read_custom_u_int256(from)?).map_err(|_| SchemaError::Invalid)
}

#[inline]
pub fn write_public_key(
    from: &PublicKey,
    to: &mut custom_u_int256::Builder,
) -> Result<(), SchemaError> {
    write_custom_u_int256(from, to)
}

#[inline]
pub fn read_rand_value(from: &custom_u_int128::Reader) -> Result<RandValue, SchemaError> {
    RandValue::from_bytes(&read_custom_u_int128(from)?).map_err(|_| SchemaError::Invalid)
}

#[inline]
pub fn write_rand_value(
    from: &RandValue,
    to: &mut custom_u_int128::Builder,
) -> Result<(), SchemaError> {
    write_custom_u_int128(from, to)
}

#[inline]
pub fn read_dh_public_key(from: &custom_u_int256::Reader) -> Result<DhPublicKey, SchemaError> {
    DhPublicKey::from_bytes(&read_custom_u_int256(from)?).map_err(|_| SchemaError::Invalid)
}

#[inline]
pub fn write_dh_public_key(
    from: &DhPublicKey,
    to: &mut custom_u_int256::Builder,
) -> Result<(), SchemaError> {
    write_custom_u_int256(from, to)
}

#[inline]
pub fn read_salt(from: &custom_u_int256::Reader) -> Result<Salt, SchemaError> {
    Salt::from_bytes(&read_custom_u_int256(from)?).map_err(|_| SchemaError::Invalid)
}

#[inline]
pub fn write_salt(from: &Salt, to: &mut custom_u_int256::Builder) -> Result<(), SchemaError> {
    write_custom_u_int256(from, to)
}

#[inline]
pub fn read_signature(from: &custom_u_int512::Reader) -> Result<Signature, SchemaError> {
    Signature::from_bytes(&read_custom_u_int512(from)?).map_err(|_| SchemaError::Invalid)
}

#[inline]
pub fn write_signature(
    from: &Signature,
    to: &mut custom_u_int512::Builder,
) -> Result<(), SchemaError> {
    write_custom_u_int512(from, to)
}

#[inline]
pub fn read_public_key_list<'a>(
    from: &struct_list::Reader<'a, custom_u_int256::Owned>,
) -> Result<Vec<PublicKey>, SchemaError> {
    let mut public_keys = Vec::with_capacity(from.len() as usize);

    for public_key_reader in from.iter() {
        public_keys.push(read_public_key(&public_key_reader)?);
    }

    Ok(public_keys)
}

#[inline]
pub fn write_public_key_list<'a>(
    from: &[PublicKey],
    to: &mut struct_list::Builder<'a, custom_u_int256::Owned>,
) -> Result<(), SchemaError> {
    debug_assert_eq!(from.len(), to.len() as usize);

    for (idx, ref_public_key) in from.iter().enumerate() {
        let mut public_key_writer = to.borrow().get(idx as u32);
        write_public_key(ref_public_key, &mut public_key_writer)?;
    }

    Ok(())
}

#[inline]
pub fn read_indexing_provider_id(
    from: &custom_u_int128::Reader,
) -> Result<IndexingProviderId, SchemaError> {
    let id_bytes = read_custom_u_int128(from)?;

    IndexingProviderId::from_bytes(&id_bytes).map_err(|_| SchemaError::Invalid)
}

#[inline]
pub fn write_indexing_provider_id(
    from: &IndexingProviderId,
    to: &mut custom_u_int128::Builder,
) -> Result<(), SchemaError> {
    write_custom_u_int128(from, to)
}

#[inline]
pub fn read_indexing_provider_state_hash(
    from: &custom_u_int256::Reader,
) -> Result<IndexingProviderStateHash, SchemaError> {
    let state_hash_bytes = read_custom_u_int256(from)?;

    IndexingProviderStateHash::from_bytes(&state_hash_bytes).map_err(|_| SchemaError::Invalid)
}

#[inline]
pub fn write_indexing_provider_state_hash(
    from: &IndexingProviderStateHash,
    to: &mut custom_u_int256::Builder,
) -> Result<(), SchemaError> {
    write_custom_u_int256(from, to)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_custom_u_int128() {
        let mut message = ::capnp::message::Builder::new_default();

        let in_buf = Bytes::from_static(&[
            0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d,
            0x0e, 0x0f,
        ]);

        assert_eq!(in_buf.len(), 16);

        let mut num_u128 = message.init_root::<custom_u_int128::Builder>();
        write_custom_u_int128(&in_buf, &mut num_u128).unwrap();

        let out_buf = read_custom_u_int128(&num_u128.borrow_as_reader()).unwrap();

        assert_eq!(&in_buf, &out_buf);
    }

    #[test]
    fn test_custom_u_int256() {
        let mut message = ::capnp::message::Builder::new_default();

        let in_buf = Bytes::from_static(&[
            0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d,
            0x0e, 0x0f, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b,
            0x1c, 0x1d, 0x1e, 0x1f,
        ]);
        assert_eq!(in_buf.len(), 32);

        let mut num_u256 = message.init_root::<custom_u_int256::Builder>();
        write_custom_u_int256(&in_buf, &mut num_u256).unwrap();

        let out_buf = read_custom_u_int256(&num_u256.borrow_as_reader()).unwrap();

        assert_eq!(&in_buf, &out_buf);
    }

    #[test]
    fn test_custom_u_int512() {
        let mut message = ::capnp::message::Builder::new_default();

        let in_buf = Bytes::from_static(&[
            0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d,
            0x0e, 0x0f, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b,
            0x1c, 0x1d, 0x1e, 0x1f, 0x20, 0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x28, 0x29,
            0x2a, 0x2b, 0x2c, 0x2d, 0x2e, 0x2f, 0x30, 0x31, 0x32, 0x33, 0x34, 0x35, 0x36, 0x37,
            0x38, 0x39, 0x3a, 0x3b, 0x3c, 0x3d, 0x3e, 0x3f,
        ]);
        assert_eq!(in_buf.len(), 64);

        let mut num_u512 = message.init_root::<custom_u_int512::Builder>();
        write_custom_u_int512(&in_buf, &mut num_u512).unwrap();

        let out_buf = read_custom_u_int512(&num_u512.borrow_as_reader()).unwrap();

        assert_eq!(&in_buf, &out_buf);
    }
}
