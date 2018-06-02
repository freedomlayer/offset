use std::io;
use std::convert::TryFrom;

use bytes::{Buf, BufMut, Bytes, BytesMut};
use byteorder::BigEndian;
use capnp::struct_list;

use crypto::rand_values::RandValue;
use crypto::dh::{DhPublicKey, Salt};
use crypto::hash::HashResult;
use crypto::identity::{PublicKey, Signature};

use proto::ProtoError;
use proto::channeler::ChannelId;
use proto::indexer::{IndexingProviderId, IndexingProviderStateHash};

include_schema!(common_capnp, "common_capnp");

const CUSTOM_UINT128_LEN: usize = 16;
const CUSTOM_UINT256_LEN: usize = 32;
const CUSTOM_UINT512_LEN: usize = 64;

/// Read the underlying bytes from given `CustomUInt128` reader.
#[inline]
pub fn read_custom_u_int128(from: &custom_u_int128::Reader) -> Result<Bytes, ProtoError> {
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
) -> Result<(), ProtoError> {
    let mut reader = io::Cursor::new(from.as_ref());

    to.set_x0(reader.get_u64::<BigEndian>());
    to.set_x1(reader.get_u64::<BigEndian>());

    Ok(())
}

/// Read the underlying bytes from given `CustomUInt256` reader.
#[inline]
pub fn read_custom_u_int256(from: &custom_u_int256::Reader) -> Result<Bytes, ProtoError> {
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
) -> Result<(), ProtoError> {
    let mut reader = io::Cursor::new(from.as_ref());

    to.set_x0(reader.get_u64::<BigEndian>());
    to.set_x1(reader.get_u64::<BigEndian>());
    to.set_x2(reader.get_u64::<BigEndian>());
    to.set_x3(reader.get_u64::<BigEndian>());

    Ok(())
}

/// Read the underlying bytes from given `CustomUInt512` reader.
#[inline]
pub fn read_custom_u_int512(from: &custom_u_int512::Reader) -> Result<Bytes, ProtoError> {
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
) -> Result<(), ProtoError> {
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

#[inline]
pub fn read_public_key(from: &custom_u_int256::Reader) -> Result<PublicKey, ProtoError> {
    PublicKey::try_from(&read_custom_u_int256(from)?).map_err(|_| ProtoError::Invalid)
}

#[inline]
pub fn write_public_key(
    from: &PublicKey,
    to: &mut custom_u_int256::Builder,
) -> Result<(), ProtoError> {
    write_custom_u_int256(from, to)
}

#[inline]
pub fn read_rand_value(from: &custom_u_int128::Reader) -> Result<RandValue, ProtoError> {
    RandValue::try_from(&read_custom_u_int128(from)?).map_err(|_| ProtoError::Invalid)
}

#[inline]
pub fn write_rand_value(
    from: &RandValue,
    to: &mut custom_u_int128::Builder,
) -> Result<(), ProtoError> {
    write_custom_u_int128(from, to)
}

#[inline]
pub fn read_dh_public_key(from: &custom_u_int256::Reader) -> Result<DhPublicKey, ProtoError> {
    DhPublicKey::try_from(&read_custom_u_int256(from)?).map_err(|_| ProtoError::Invalid)
}

#[inline]
pub fn write_dh_public_key(
    from: &DhPublicKey,
    to: &mut custom_u_int256::Builder,
) -> Result<(), ProtoError> {
    write_custom_u_int256(from, to)
}

#[inline]
pub fn read_salt(from: &custom_u_int256::Reader) -> Result<Salt, ProtoError> {
    Salt::try_from(&read_custom_u_int256(from)?).map_err(|_| ProtoError::Invalid)
}

#[inline]
pub fn write_salt(from: &Salt, to: &mut custom_u_int256::Builder) -> Result<(), ProtoError> {
    write_custom_u_int256(from, to)
}

#[inline]
pub fn read_signature(from: &custom_u_int512::Reader) -> Result<Signature, ProtoError> {
    Signature::try_from(&read_custom_u_int512(from)?).map_err(|_| ProtoError::Invalid)
}

#[inline]
pub fn write_signature(
    from: &Signature,
    to: &mut custom_u_int512::Builder,
) -> Result<(), ProtoError> {
    write_custom_u_int512(from, to)
}

#[inline]
pub fn read_public_key_list<'a>(
    from: &struct_list::Reader<'a, custom_u_int256::Owned>,
) -> Result<Vec<PublicKey>, ProtoError> {
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
) -> Result<(), ProtoError> {
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
) -> Result<IndexingProviderId, ProtoError> {
    let id_bytes = read_custom_u_int128(from)?;

    IndexingProviderId::try_from(id_bytes.as_ref()).map_err(|_| ProtoError::Invalid)
}

#[inline]
pub fn write_indexing_provider_id(
    from: &IndexingProviderId,
    to: &mut custom_u_int128::Builder,
) -> Result<(), ProtoError> {
    write_custom_u_int128(from, to)
}

#[inline]
pub fn read_indexing_provider_state_hash(
    from: &custom_u_int256::Reader,
) -> Result<IndexingProviderStateHash, ProtoError> {
    let state_hash_bytes = read_custom_u_int256(from)?;

    IndexingProviderStateHash::try_from(state_hash_bytes.as_ref()).map_err(|_| ProtoError::Invalid)
}

#[inline]
pub fn write_indexing_provider_state_hash(
    from: &IndexingProviderStateHash,
    to: &mut custom_u_int256::Builder,
) -> Result<(), ProtoError> {
    write_custom_u_int256(from, to)
}

#[inline]
pub fn write_hash_result(
    from: &HashResult,
    to: &mut custom_u_int256::Builder,
) -> Result<(), ProtoError> {
    write_custom_u_int256(from, to)
}

#[inline]
pub fn read_hash_result(
    from: &custom_u_int256::Reader
) -> Result<HashResult, ProtoError> {
    let hash_result_bytes = read_custom_u_int256(from)?;

    HashResult::try_from(hash_result_bytes.as_ref()).map_err(|_| ProtoError::Invalid)
}

#[inline]
pub fn read_channel_id(
    from: &custom_u_int128::Reader
) -> Result<ChannelId, ProtoError> {
    let channel_id_bytes = read_custom_u_int128(from)?;

    ChannelId::try_from(channel_id_bytes.as_ref()).map_err(|_| ProtoError::Invalid)
}

#[inline]
pub fn write_channel_id(
    from: &ChannelId,
    to: &mut custom_u_int128::Builder
) -> Result<(), ProtoError> {
    write_custom_u_int128(from, to)
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
